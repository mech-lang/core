use table::{Table, TableId, TableIndex};
use value::{Value, ValueMethods};
use index::{ValueIterator, TableIterator, IndexIterator, AliasIterator};
use database::{Database, Store, Change, Transaction};
use hashbrown::{HashMap, HashSet};
use quantities::{QuantityMath, make_quantity};
use operations::{MechFunction, resolve_subscript};
use errors::{ErrorType};
use std::cell::RefCell;
use std::sync::Arc;
use rust_core::fmt;
use ::humanize;
use ::hash_string;

lazy_static! {
  static ref TABLE_SPLIT: u64 = hash_string("table/split");
  static ref GRAMS: u64 = hash_string("g");
  static ref KILOGRAMS: u64 = hash_string("kg");
}

// ## Block

// Blocks are the ubiquitous unit of code in a Mech program. Users do not write functions in Mech, as in
// other languages. Blocks consist of a number of "Transforms" that read values from tables and reshape
// them or perform computations on them. Blocks can be thought of as pure functions where the input and
// output are tables. Blocks have their own internal table store. Local tables can be defined within a
// block, which allows the programmer to break a computation down into steps. The result of the computation
// is then output to one or more global tables, which triggers the execution of other blocks in the network.
#[derive(Clone)]
pub struct Block {
  pub id: u64,
  pub state: BlockState,
  pub text: String,
  pub name: String,
  pub ready: HashSet<Register>,
  pub input: HashSet<Register>,
  pub output: HashSet<Register>,
  pub output_dependencies: HashSet<Register>,
  pub output_dependencies_ready: HashSet<Register>,
  pub register_aliases: HashMap<Register, HashSet<Register>>,
  pub tables: HashMap<u64, Table>,
  pub store: Arc<Store>,
  pub transformations: Vec<(String, Vec<Transformation>)>,
  pub plan: Vec<Transformation>,
  pub changes: Vec<Change>,
  pub errors: Vec<ErrorType>,
  pub triggered: usize,
}

impl Block {
  pub fn new(capacity: usize) -> Block {
    Block {
      id: 0,
      text: String::new(),
      name: String::new(),
      ready: HashSet::new(),
      input: HashSet::new(),
      output: HashSet::new(),
      output_dependencies: HashSet::new(),
      output_dependencies_ready: HashSet::new(),
      register_aliases: HashMap::new(),
      state: BlockState::New,
      tables: HashMap::new(),
      store: Arc::new(Store::new(capacity)),
      transformations: Vec::new(),
      plan: Vec::new(),
      changes: Vec::new(),
      errors: Vec::new(),
      triggered: 0,
    }
  }

  pub fn gen_id(&mut self) {
    let mut words = "".to_string();
    for tfm in &self.transformations {
      words = format!("{:?}{:?}", words, tfm);
    }
    self.id = seahash::hash(words.as_bytes()) & 0x00FFFFFFFFFFFFFF;
  }

  pub fn register_transformations(&mut self, tfm_tuple: (String, Vec<Transformation>)) {
    self.transformations.push(tfm_tuple.clone());

    let (_, transformations) = tfm_tuple;

    for tfm in transformations {
      match tfm {
        Transformation::NewTable{table_id, rows, columns} => {
          match table_id {
            TableId::Global(id) => {
              self.changes.push(
                Change::NewTable{
                  table_id: id,
                  rows,
                  columns,
                }
              );
              let register_all = Register{table_id, row: TableIndex::All, column: TableIndex::All};
              self.output.insert(register_all);
            }
            TableId::Local(id) => {
              self.tables.insert(id, Table::new(id, rows, columns, self.store.clone()));
            }
          }
        }
        Transformation::ColumnAlias{table_id, column_ix, column_alias} => {
          match table_id {
            TableId::Global(id) => {
              self.changes.push(
                Change::SetColumnAlias{
                  table_id: id,
                  column_ix,
                  column_alias,
                }
              );
              let register_all = Register{table_id: table_id, row: TableIndex::All, column: TableIndex::All};
              let register_alias = Register{table_id: table_id, row: TableIndex::All, column: TableIndex::Alias(column_alias)};
              let register_ix = Register{table_id: table_id, row: TableIndex::All, column: TableIndex::Index(column_ix)};
              // Alias mappings
              let aliases = self.register_aliases.entry(register_alias).or_insert(HashSet::new());
              aliases.insert(register_ix);
              aliases.insert(register_all);
              // Index mappings
              let aliases = self.register_aliases.entry(register_ix).or_insert(HashSet::new());
              aliases.insert(register_alias);
              aliases.insert(register_all);
              // All mappings
              let aliases = self.register_aliases.entry(register_all).or_insert(HashSet::new());
              aliases.insert(register_ix);
              aliases.insert(register_alias);
              self.output.insert(register_alias);              
            }
            TableId::Local(_id) => {
              let store = unsafe{&mut *Arc::get_mut_unchecked(&mut self.store)};
              store.column_index_to_alias.insert((*table_id.unwrap(),column_ix),column_alias);
              store.column_alias_to_index.insert((*table_id.unwrap(),column_alias),column_ix);
            }
          }
        }
        Transformation::Constant{table_id, value, unit} => {
          let (domain, scale) = if unit == *GRAMS { (1, 0) }
            else if unit            == *KILOGRAMS { (1, 3) }
//              "m" => (2, 0),
//              "km" => (2, 3),
//              "ms" => (3, 0),
//              "s" => (3, 3),
              else { (0, 0) };
          let q = if value.is_number() {
            Value::from_quantity(make_quantity(value.mantissa(), value.range() + scale, domain))
          } else {
            value
          };
          match table_id {
            TableId::Local(id) => {
              let table = self.tables.get_mut(&id).unwrap();
              table.set(&TableIndex::Index(1), &TableIndex::Index(1), q);
            }
            TableId::Global(id) => {
              self.changes.push(
                Change::Set{
                  table_id: id,
                  values: vec![(TableIndex::Index(1), TableIndex::Index(1), q)],
                }
              );
            }
           // _ => (),
          }
        }
        Transformation::Set{table_id, row: _, column: _} => {
          let register_all = Register{table_id: table_id, row: TableIndex::All, column: TableIndex::All};
          self.output.insert(register_all);       
          self.output_dependencies.insert(register_all);          
        }
        Transformation::Whenever{table_id, registers, ..} => {
          let whenever_ix_table_id = hash_string("~");
          self.tables.insert(whenever_ix_table_id, Table::new(whenever_ix_table_id, 0, 1, self.store.clone()));
          match table_id {
            TableId::Global(_id) => {
              for register in registers {
                self.input.insert(register);
              }
            }
            _ => (),
          }
        }
        Transformation::Function{ref arguments, out, ..} => {
          let (out_id, row, column) = out;
          match out_id {
            TableId::Global(_id) => {
              let row = match row {
                TableIndex::Table(_) => TableIndex::All,
                x => x,
              };
              let column = match column {
                TableIndex::Table(_) => TableIndex::All,
                x => x,
              };
              let register = Register{table_id: out_id, row, column};
              self.output.insert(register);
              let register = Register{table_id: out_id, row: TableIndex::All, column: TableIndex::All};
              self.output.insert(register);
            },
            _ => (),
          }
          for (_, table_id, row, column) in arguments {
            match table_id {
              TableId::Global(_id) => {
                let row2: &TableIndex = match row {
                  TableIndex::Table{..} => &TableIndex::All,
                  TableIndex::None => &TableIndex::All,
                  x => x,
                };
                let column2: &TableIndex = match column {
                  TableIndex::Table{..} => &TableIndex::All,
                  TableIndex::None => &TableIndex::All,
                  x => x,
                };
                let register_ix = Register{table_id: *table_id, row: *row2, column: *column2};
                let register_all = Register{table_id: *table_id, row: TableIndex::All, column: TableIndex::All};
                let aliases = self.register_aliases.entry(register_ix).or_insert(HashSet::new());
                aliases.insert(register_all);
                let aliases = self.register_aliases.entry(register_all).or_insert(HashSet::new());
                aliases.insert(register_ix);
                self.input.insert(register_ix);
              },
              _ => (),
            }
          }
        }
        _ => (),
      }
    }
  }

  // Process changes queued on the block
  pub fn process_changes(&mut self, database: Arc<RefCell<Database>>) {
    if !self.changes.is_empty() {
      let txn = Transaction {
        changes: self.changes.clone(),
      };
      self.changes.clear();
      database.borrow_mut().process_transaction(&txn).ok();
      database.borrow_mut().transactions.push(txn);
    }
  }

  pub fn solve(&mut self, database: Arc<RefCell<Database>>, functions: &HashMap<u64, Option<MechFunction>>) {
    self.triggered += 1;
    'step_loop: for step in &self.plan {
      match step {
        Transformation::Whenever{table_id, registers, ..} => {
          let register = registers[0];
          // Resolve whenever table subscript so we can iterate through the values
          let mut vi = resolve_subscript(register.table_id,register.row,register.column,&mut self.tables, &database);
          // Get the whenever table from the local store
          let whenever_ix_table_id = hash_string("~");
          let mut whenever_table = self.tables.get_mut(&whenever_ix_table_id).unwrap();
          // Check to see if the whenever table needs to be resized
          let before_rows = whenever_table.rows;
          if vi.rows() > whenever_table.rows {
            whenever_table.resize(vi.rows(),1);
            for (ix, (_, changed)) in vi.enumerate() {
              // Mark the new rows as changed even if they are stale
              if ix+1 > before_rows {
                whenever_table.set_unchecked(ix+1, 1, Value::from_bool(true));
              // Use the changed value of old rows
              } else {
                whenever_table.set_unchecked(ix+1, 1, Value::from_bool(changed));
              }
            }
          // If the table hasn't been resized, use the changed value
          } else {
            for (ix, (_, changed)) in vi.enumerate() {
              whenever_table.set_unchecked(ix+1, 1, Value::from_bool(changed));
            }
          }

          // If all of the rows of the whenever table are false, there is nothing for this block to do
          // because none of the values it is watching have changed
          let mut flag = false;
          for ix in 1..=whenever_table.rows {
            let (val, _) = whenever_table.get_unchecked(ix,1);
            match val.as_bool() {
              Some(true) => flag = true,
              _ => (),
            }
          }
          if flag == false {
            break 'step_loop;
          }
          
          match table_id {
            TableId::Global(_id) => {
              for register in registers {
                self.ready.remove(&register);
              }
            }
            TableId::Local(id) => {
              let mut flag = false;
              let table = self.tables.get_mut(&id).unwrap() as *mut Table;
              unsafe {
                for i in 1..=(*table).rows {
                  for j in 1..=(*table).columns {
                    let (val, _) = (*table).get_unchecked(i,j);
                    match val.as_bool() {
                      Some(true) => flag = true,
                      _ => (),
                    }
                  }
                }
              }
              if flag == false {
                break 'step_loop;
              } else {
                for register in registers {
                  self.ready.remove(&register);
                }
              }
            },
          }
        },
        Transformation::Select{table_id, row, column, indices, out} => {
          let mut db = database.borrow_mut();
          let mut table_id = table_id;
        
          let mut table = match table_id {
            TableId::Global(id) => db.tables.get_mut(&id).unwrap() as *mut Table,
            TableId::Local(id) => self.tables.get_mut(&id).unwrap() as *mut Table,
          };

          let mut out_table = match out {
            TableId::Global(id) => db.tables.get_mut(&id).unwrap() as *mut Table,
            TableId::Local(id) => self.tables.get_mut(&id).unwrap() as *mut Table,
          };
        
          /*unsafe{
            if (*table).rows == 1 && (*table).columns == 1 {
              match (row_index, column_index) {
                (TableIndex::All, TableIndex::All) => (),
                (_, _) => {
                  let (reference, _) = (*table).get_unchecked(1,1);
                  match reference.as_reference() {
                    Some(table_reference) => {
                      match db.tables.get_mut(&table_reference) {
                        Some(dbtable) => table = dbtable as *mut Table,
                        None => (),
                      }
                    }
                    _ => (),
                  }
                }
              }
            }
            table_id = TableId::Global((*table).id);
          }*/
          
          for (ix, (row_index, column_index)) in indices.iter().enumerate() {
            let row_iter = unsafe { match row_index {
              TableIndex::Index(ix) => IndexIterator::Constant(TableIndex::Index(*ix)),
              TableIndex::All => {
                match (*table).rows {
                  0 => IndexIterator::None,
                  r => IndexIterator::Range(1..=r),
                }
              },
              TableIndex::Table(table_id) => {
                let row_table = match table_id {
                  TableId::Global(id) => db.tables.get_mut(&id).unwrap() as *mut Table,
                  TableId::Local(id) => self.tables.get_mut(&id).unwrap() as *mut Table,
                };
                IndexIterator::Table(TableIterator::new(row_table))
              }
              TableIndex::Alias(alias) => IndexIterator::Alias(AliasIterator::new(*alias, *table_id, db.store.clone())),
              _ => IndexIterator::Range(1..=(*table).rows),
            }};
          
            let column_iter = unsafe { match column_index {
              TableIndex::Index(ix) => IndexIterator::Constant(TableIndex::Index(*ix)),
              TableIndex::All => {
                match (*table).columns {
                  0 => IndexIterator::None,
                  c => IndexIterator::Range(1..=c),
                }
              }
              TableIndex::Table(table_id) => {
                let col_table = match table_id {
                  TableId::Global(id) => db.tables.get_mut(&id).unwrap() as *mut Table,
                  TableId::Local(id) => self.tables.get_mut(&id).unwrap() as *mut Table,
                };
                IndexIterator::Table(TableIterator::new(col_table))
              }
              TableIndex::Alias(alias) => IndexIterator::Alias(AliasIterator::new(*alias, *table_id, self.store.clone())),
              TableIndex::None => IndexIterator::Constant(TableIndex::Index(0)),
              //_ => IndexIterator::Range(1..=(*table).columns),
            }};
            let mut vi = ValueIterator{
              scope: *table_id,
              table,
              row_index: *row_index,
              column_index: *column_index,
              row_iter,
              column_iter,
            };
            // If this is the last index, then we can write the data to the output table
            if ix == indices.len() - 1 {
              unsafe{(*out_table).resize(vi.rows(), vi.columns())};
              let mut out_ix = 1;
              for (value, _) in vi {
                println!("{:?} {:?}", ix, value);
                unsafe{(*out_table).set_unchecked_linear(out_ix, value);}
                out_ix += 1;
              }
            }
          }




        }
        Transformation::Function{name, arguments, out} => {
          let mut vis: Vec<(u64, ValueIterator)> = vec![];
          for (arg, table, row, column) in arguments {
            let vi = resolve_subscript(*table,*row,*column,&mut self.tables, &database);
            vis.push((arg.clone(),vi));
          }
          let (out_table_id, out_row, out_column) = out;
          let mut out_vi = resolve_subscript(*out_table_id, *out_row, *out_column, &mut self.tables, &database);
          match functions.get(name) {
            Some(Some(mech_fn)) => {
              mech_fn(&vis, &mut out_vi);
            }
            _ => {
              if *name == *TABLE_SPLIT {
                let (_, vi) = &vis[0];
                let vi_table = unsafe{&(*vi.table)};

                unsafe{ (*out_vi.table).resize(vi.rows(), 1); }

                for row in vi.row_iter.clone() {
                  let old_table_id = unsafe{(*vi.table).id};
                  let new_table_id = hash_string(&format!("{:?}{:?}",old_table_id,row));
                  let columns = vi.columns().clone();
                  let mut table = Table::new(new_table_id,1,columns,self.store.clone());
                  for column in vi.column_iter.clone() {
                    let value = vi.get(&row,&column).unwrap();
                    table.set(&TableIndex::Index(1),&column, value);
                  }
                  self.tables.insert(new_table_id, table);
                  unsafe {
                    (*out_vi.table).set(&row,&TableIndex::Index(1),Value::from_id(new_table_id));
                  }
                  let txn = Transaction {
                    changes: vec![Change::NewTable{
                      table_id: new_table_id,
                      rows: 1,
                      columns: vi.columns(),
                    }],
                  };
                  self.changes.clear();
                  let mut db = database.borrow_mut();
                  db.process_transaction(&txn).ok();
                  db.transactions.push(txn);
                  let new_global_copy_table = db.tables.get_mut(&new_table_id).unwrap() as *mut Table;
                  unsafe {
                    for i in 1..=vi.columns() {
                      // Add alias to column if it's there
                      match vi_table.store.column_index_to_alias.get(&(vi_table.id,i)) {
                        Some(alias) => {
                          let out_id = (*new_global_copy_table).id;
                          let store = &mut *Arc::get_mut_unchecked(&mut (*new_global_copy_table).store);
                          store.column_index_to_alias.entry((out_id,i)).or_insert(*alias);
                          store.column_alias_to_index.entry((out_id,*alias)).or_insert(i);
                        }
                        _ => (),
                      }
                      let (val, _) = vi_table.get_unchecked(row.unwrap(),i);
                      (*new_global_copy_table).set_unchecked(1,i, val);
                    }
                  }
                }
              } else {
                // TODO Error: Function not found
                //println!("Function not found {:?}", humanize(name));
                return;
              }
            },
          }
        }
        _ => (),
      }
    }
    self.state = BlockState::Done
  }

  pub fn is_ready(&mut self) -> bool {
    // The block will not execute if it's in an error state or disabled
    if self.state == BlockState::Error || self.state == BlockState::Disabled {
      false
    // The block will not execute if there are any errors listed on it
    } else if self.errors.len() > 0 {
      self.state = BlockState::Error;
      false
    } else {
      // The block is ready if the ready output and input registers equal the total
      if self.ready.len() < self.input.len() || self.output_dependencies_ready.len() < self.output_dependencies.len() {
        false
      } else {
        self.state = BlockState::Ready;
        true
      }
    }
  }

}

impl fmt::Debug for Block {
  #[inline]
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "┌─────────────────────────────────────────────┐\n")?;
    write!(f, "│ id: {}\n", humanize(&self.id))?;
    write!(f, "│ state: {:?}\n", self.state)?;
    write!(f, "│ triggered: {:?}\n", self.triggered)?;
    write!(f, "├─────────────────────────────────────────────┤\n")?;
    write!(f, "│ errors: {}\n", self.errors.len())?;
    for (ix, error) in self.errors.iter().enumerate() {
      write!(f, "│    {}. {:?}\n", ix+1, error)?;
    }
    write!(f, "├─────────────────────────────────────────────┤\n")?;
    write!(f, "│ ready: {}\n", self.ready.len())?;
    for (ix, register) in self.ready.iter().enumerate() {
      write!(f, "│    {}. {}\n", ix+1, format_register(&self.store.strings, register))?;
    }
    write!(f, "│ input: {} \n", self.input.len())?;
    for (ix, register) in self.input.iter().enumerate() {
      write!(f, "│    {}. {}\n", ix+1, format_register(&self.store.strings, register))?;
    }
    if self.ready.len() < self.input.len() {
      write!(f, "│ missing: \n")?;
      for (ix, register) in self.input.difference(&self.ready).enumerate() {
        write!(f, "│    {}. {}\n", ix+1, format_register(&self.store.strings, register))?;
      }
    }
    write!(f, "│ output: {}\n", self.output.len())?;
    for (ix, register) in self.output.iter().enumerate() {
      write!(f, "│    {}. {}\n", ix+1, format_register(&self.store.strings, register))?;
    }
    write!(f, "│ output dep: {}\n", self.output_dependencies.len())?;
    for (ix, register) in self.output_dependencies.iter().enumerate() {
      write!(f, "│    {}. {}\n", ix+1, format_register(&self.store.strings, register))?;
    }
    write!(f, "│ output ready: {}\n", self.output_dependencies_ready.len())?;
    for (ix, register) in self.output_dependencies_ready.iter().enumerate() {
      write!(f, "│    {}. {}\n", ix+1, format_register(&self.store.strings, register))?;
    }
    write!(f, "├─────────────────────────────────────────────┤\n")?;
    write!(f, "│ transformations: \n")?;
    for (ix, (text, tfms)) in self.transformations.iter().enumerate() {
      write!(f, "│  {}. {}\n", ix+1, text)?;
      for tfm in tfms {
        let tfm_string = format_transformation(&self,&tfm);
        write!(f, "│       > {}\n", tfm_string)?;
      }
    }
    write!(f, "│ plan: \n")?;
    for (ix, tfm) in self.plan.iter().enumerate() {
      let tfm_string = format_transformation(&self,tfm);
      write!(f, "│    {}. {}\n", ix+1, tfm_string)?;
    }
    write!(f, "│ tables: {} \n", self.tables.len())?;
    for (_, table) in self.tables.iter() {
      write!(f, "{:?}\n", table)?;
    }

    Ok(())
  }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockState {
  New,          // Has just been created, but has not been tested for satisfaction
  Ready,        // All inputs are satisfied and the block is ready to execute
  Done,         // All inputs are satisfied and the block has executed
  Unsatisfied,  // One or more inputs are not satisfied
  Error,        // One or more errors exist on the block
  Disabled,     // The block is disabled will not execute if it otherwise would
}

pub enum Error {
  TableNotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transformation {
  NewTable{table_id: TableId, rows: usize, columns: usize },
  Constant{table_id: TableId, value: Value, unit: u64},
  ColumnAlias{table_id: TableId, column_ix: usize, column_alias: u64},
  Set{table_id: TableId, row: TableIndex, column: TableIndex},
  RowAlias{table_id: TableId, row_ix: usize, row_alias: u64},
  Whenever{table_id: TableId, row: TableIndex, column: TableIndex, registers: Vec<Register>},
  Function{name: u64, arguments: Vec<(u64, TableId, TableIndex, TableIndex)>, out: (TableId, TableIndex, TableIndex)},
  Select{table_id: TableId, row: TableIndex, column: TableIndex, indices: Vec<(TableIndex, TableIndex)>, out: TableId},
}

#[derive(Debug, Copy, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct Register {
  pub table_id: TableId,
  pub row: TableIndex,
  pub column: TableIndex,
}

impl Register {
  pub fn hash(&self) -> u64 {
    let id_bytes = (*self.table_id.unwrap()).to_le_bytes();

    let unwrap_index = |index: &TableIndex| -> u64 {
      match index {
        TableIndex::Index(ix) => *ix as u64,
        TableIndex::Alias(alias) => {
          alias.clone()
        },
        TableIndex::Table(table_id) => *table_id.unwrap(),
        TableIndex::None |
        TableIndex::All => 0,
      }
    };

    let row_bytes = unwrap_index(&self.row).to_le_bytes();
    let column_bytes = unwrap_index(&self.column).to_le_bytes();
    let array = [id_bytes, row_bytes, column_bytes].concat();
    seahash::hash(&array) & 0x00FFFFFFFFFFFFFF
  }
}

pub fn format_register(strings: &HashMap<u64, String>, register: &Register) -> String {

  let table_id = register.table_id;
  let row = register.row;
  let column = register.column;
  let mut arg = format!("");
  match table_id {
    TableId::Global(id) => {
      let name = match strings.get(&id) {
        Some(name) => name.clone(),
        None => format!("{:}",humanize(&id)),
      };
      arg=format!("{}#{}",arg,name)
    },
    TableId::Local(id) => {
      match strings.get(&id) {
        Some(name) => arg = format!("{}{}",arg,name),
        None => arg = format!("{}{}",arg,humanize(&id)),
      }
    }
  };
  match row {
    TableIndex::None => arg=format!("{}{{-,",arg),
    TableIndex::All => arg=format!("{}{{:,",arg),
    TableIndex::Index(ix) => arg=format!("{}{{{},",arg,ix),
    TableIndex::Table(table) => {
      match table {
        TableId::Global(id) => arg=format!("{}#{}",arg,strings.get(&id).unwrap()),
        TableId::Local(id) => {
          match strings.get(&id) {
            Some(name) => arg = format!("{}{}",arg,name),
            None => arg = format!("{}{}",arg,humanize(&id)),
          }
        }
      };
    }
    TableIndex::Alias(alias) => {
      let alias_name = strings.get(&alias).unwrap();
      arg=format!("{}{{{},",arg,alias_name);
    },
  }
  match column {
    TableIndex::None => arg=format!("{}-}}",arg),
    TableIndex::All => arg=format!("{}:}}",arg),
    TableIndex::Index(ix) => arg=format!("{}{}}}",arg,ix),
    TableIndex::Table(table) => {
      match table {
        TableId::Global(id) => arg=format!("{}#{}",arg,strings.get(&id).unwrap()),
        TableId::Local(id) => {
          match strings.get(&id) {
            Some(name) => arg = format!("{}{}",arg,name),
            None => arg = format!("{}{}",arg,humanize(&id)),
          }
        }
      };
    }
    TableIndex::Alias(alias) => {
      match strings.get(&alias) {
        Some(alias_name) => arg=format!("{}{}}}",arg,alias_name),
        None => arg=format!("{}{}}}",arg,&humanize(&alias)),
      };
      
    },
  }
  arg

}

fn format_transformation(block: &Block, tfm: &Transformation) -> String {
  match tfm {
    Transformation::NewTable{table_id, rows, columns} => {
      let mut tfm = format!("table/new(");
      match table_id {
        TableId::Global(id) => {
          let name = match block.store.strings.get(id) {
            Some(name) => name.clone(),
            None => format!("{:}",humanize(id)),
          };
          tfm=format!("{}#{}",tfm,name);
        }
        TableId::Local(id) => {
          match block.store.strings.get(id) {
            Some(name) =>  tfm=format!("{}{}",tfm,name),
            None => tfm=format!("{}{}",tfm,humanize(id)),
          }
        }
      };
      tfm = format!("{} {} x {})",tfm,rows,columns);
      tfm
    }
    Transformation::Whenever{table_id, row, column, ..} => {
      let mut arg = format!("~ ");
      match table_id {
        TableId::Global(id) => {
          let name = match block.store.strings.get(id) {
            Some(name) => name.clone(),
            None => format!("{:}",humanize(id)),
          };
          arg=format!("{}#{}",arg,name)
        },
        TableId::Local(id) => {
          match block.store.strings.get(id) {
            Some(name) => arg = format!("{}{}",arg,name),
            None => arg = format!("{}{}",arg,humanize(id)),
          }
        }
      };
      match row {
        TableIndex::None => arg=format!("{}{{-,",arg),
        TableIndex::All => arg=format!("{}{{:,",arg),
        TableIndex::Index(ix) => arg=format!("{}{{{},",arg,ix),
        TableIndex::Table(table) => {
          match table {
            TableId::Global(id) => arg=format!("{}#{}",arg,block.store.strings.get(id).unwrap()),
            TableId::Local(id) => {
              match block.store.strings.get(id) {
                Some(name) => arg = format!("{}{}",arg,name),
                None => arg = format!("{}{}",arg,humanize(id)),
              }
            }
          };
        }
        TableIndex::Alias(alias) => {
          let alias_name = block.store.strings.get(alias).unwrap();
          arg=format!("{}{{{},",arg,alias_name);
        },
      }
      match column {
        TableIndex::None => arg=format!("{}-}}",arg),
        TableIndex::All => arg=format!("{}:}}",arg),
        TableIndex::Index(ix) => arg=format!("{}{}}}",arg,ix),
        TableIndex::Table(table) => {
          match table {
            TableId::Global(id) => arg=format!("{}#{}",arg,block.store.strings.get(id).unwrap()),
            TableId::Local(id) => {
              match block.store.strings.get(id) {
                Some(name) => arg = format!("{}{}",arg,name),
                None => arg = format!("{}{}",arg,humanize(id)),
              }
            }
          };
        }
        TableIndex::Alias(alias) => {
          let alias_name = block.store.strings.get(alias).unwrap();
          arg=format!("{}{}}}",arg,alias_name);
        },
      }
      arg
    }
    Transformation::Constant{table_id, value, ..} => {
      let mut tfm = format!("const(");
      match value.as_quantity() {
        Some(_quantity) => tfm = format!("{}{:?}", tfm, value),
        None => {
          if value.is_empty() {
            tfm = format!("{} _",tfm);
          } else {
            match value.as_reference() {
              Some(_reference) => {tfm = format!("{}@{}",tfm, humanize(value));}
              None => {
                match value.as_bool() {
                  Some(true) => tfm = format!("{} true",tfm),
                  Some(false) => tfm = format!("{} false",tfm),
                  None => {tfm = format!("{}{:?}",tfm, block.store.strings.get(value).unwrap());}
                }
              }
            }

          }
        },
      }
      tfm = format!("{}) -> ",tfm);
      match table_id {
        TableId::Global(id) => tfm=format!("{}#{}",tfm,block.store.strings.get(id).unwrap()),
        TableId::Local(id) => {
          match block.store.strings.get(id) {
            Some(name) =>  tfm=format!("{}{}",tfm,name),
            None => tfm=format!("{}{}",tfm,humanize(id)),
          }
        }
      };
      tfm
    }
    Transformation::ColumnAlias{table_id, column_ix, column_alias} => {
      let mut tfm = format!("");
      match table_id {
        TableId::Global(id) => {
          tfm = match block.store.strings.get(id) {
            Some(string) => format!("{}#{}",tfm,string),
            None => humanize(&id),
          };
        } 
        TableId::Local(id) => {
          match block.store.strings.get(id) {
            Some(name) => tfm = format!("{}{}",tfm,name),
            None => tfm = format!("{}{}",tfm,humanize(id)),
          }
        }
      }
      tfm = format!("{}({:x})",tfm,column_ix);
      tfm = format!("{} -> {}",tfm,block.store.strings.get(column_alias).unwrap());
      tfm
    }
    Transformation::Select{table_id, row, column, indices, out} => {
      let mut tfm = format!("table/select(");
      match table_id {
        TableId::Global(id) => {
          let name = match block.store.strings.get(id) {
            Some(name) => name.clone(),
            None => format!("{:}",humanize(id)),
          };
          tfm=format!("{}#{}",tfm,name)
        },
        TableId::Local(id) => {
          match block.store.strings.get(id) {
            Some(name) => tfm = format!("{}{}",tfm,name),
            None => tfm = format!("{}{}",tfm,humanize(id)),
          }
        }
      };
      for (row, column) in indices {
        match row {
          TableIndex::None => tfm=format!("{}{{-,",tfm),
          TableIndex::All => tfm=format!("{}{{:,",tfm),
          TableIndex::Index(ix) => tfm=format!("{}{{{},",tfm,ix),
          TableIndex::Table(table) => {
            match table {
              TableId::Global(id) => tfm=format!("{}#{}",tfm,block.store.strings.get(id).unwrap()),
              TableId::Local(id) => {
                match block.store.strings.get(id) {
                  Some(name) => {
                    tfm = format!("{}{{{},",tfm,name);
                  },
                  None => tfm = format!("{}{{{},",tfm,humanize(id)),
                }
              }
            };
          }
          TableIndex::Alias(alias) => {
            let alias_name = block.store.strings.get(alias).unwrap();
            tfm=format!("{}{{{},",tfm,alias_name);
          },
        }
        match column {
          TableIndex::None => tfm=format!("{}-}}",tfm),
          TableIndex::All => tfm=format!("{}:}}",tfm),
          TableIndex::Index(ix) => tfm=format!("{}{}}}",tfm,ix),
          TableIndex::Table(table) => {
            match table {
              TableId::Global(id) => tfm=format!("{}#{}",tfm,block.store.strings.get(id).unwrap()),
              TableId::Local(id) => {
                match block.store.strings.get(id) {
                  Some(name) => tfm = format!("{}{}",tfm,name),
                  None => tfm = format!("{}{}",tfm,humanize(id)),
                }
              }
            };
          }
          TableIndex::Alias(alias) => {
            let alias_name = block.store.strings.get(alias).unwrap();
            tfm=format!("{}.{}}}",tfm,alias_name);
          },
        }
      }
      tfm=format!("{}) -> {}", tfm, humanize(&out.unwrap()));
      tfm
    }
    Transformation::Function{name, arguments, out} => {
      let name_string = match block.store.strings.get(name) {
        Some(name_string) => name_string.clone(),
        None => format!("{}", humanize(name)),
      };
      let mut arg = format!("");
      for (ix,(_arg_id, table, row, column)) in arguments.iter().enumerate() {
        match table {
          TableId::Global(id) => {
            let name = match block.store.strings.get(id) {
              Some(name) => name.clone(),
              None => format!("{:}",humanize(id)),
            };
            arg=format!("{}#{}",arg,name)
          },
          TableId::Local(id) => {
            match block.store.strings.get(id) {
              Some(name) => arg = format!("{}{}",arg,name),
              None => arg = format!("{}{}",arg,humanize(id)),
            }
          }
        };
        match row {
          TableIndex::None => arg=format!("{}{{-,",arg),
          TableIndex::All => arg=format!("{}{{:,",arg),
          TableIndex::Index(ix) => arg=format!("{}{{{},",arg,ix),
          TableIndex::Table(table) => {
            match table {
              TableId::Global(id) => arg=format!("{}#{}",arg,block.store.strings.get(id).unwrap()),
              TableId::Local(id) => {
                match block.store.strings.get(id) {
                  Some(name) => {
                    arg = format!("{}{{{},",arg,name);
                  },
                  None => arg = format!("{}{{{},",arg,humanize(id)),
                }
              }
            };
          }
          TableIndex::Alias(alias) => {
            let alias_name = block.store.strings.get(alias).unwrap();
            arg=format!("{}{{{},",arg,alias_name);
          },
        }
        match column {
          TableIndex::None => arg=format!("{}-}}",arg),
          TableIndex::All => arg=format!("{}:}}",arg),
          TableIndex::Index(ix) => arg=format!("{}{}}}",arg,ix),
          TableIndex::Table(table) => {
            match table {
              TableId::Global(id) => arg=format!("{}#{}",arg,block.store.strings.get(id).unwrap()),
              TableId::Local(id) => {
                match block.store.strings.get(id) {
                  Some(name) => arg = format!("{}{}",arg,name),
                  None => arg = format!("{}{}",arg,humanize(id)),
                }
              }
            };
          }
          TableIndex::Alias(alias) => {
            let alias_name = block.store.strings.get(alias).unwrap();
            arg=format!("{}.{}}}",arg,alias_name);
          },
        }
        if ix < arguments.len()-1 {
          arg=format!("{}, ", arg);
        }
      }
      let mut arg = format!("{}({}) -> ",name_string,arg);
      let (out_table, out_row, out_column) = out;
      match out_table {
        TableId::Global(id) => {
          let name = match block.store.strings.get(id) {
            Some(name) => name.clone(),
            None => format!("{:}",humanize(id)),
          };
          arg=format!("{}#{}",arg,name);
        }
        TableId::Local(id) => {
          match block.store.strings.get(id) {
            Some(name) => arg = format!("{}{}",arg,name),
            None => arg = format!("{}{}",arg,humanize(id)),
          }
        }
      };
      match out_row {
        TableIndex::None => arg=format!("{}{{-,",arg),
        TableIndex::All => arg=format!("{}{{:,",arg),
        TableIndex::Index(ix) => arg=format!("{}{{{},",arg,ix),
        TableIndex::Table(table) => {
          match table {
            TableId::Global(id) => arg=format!("{}{{#{},",arg,block.store.strings.get(id).unwrap()),
            TableId::Local(id) => {
              match block.store.strings.get(id) {
                Some(name) => arg = format!("{}{{{},",arg,name),
                None => arg = format!("{}{{{},",arg,humanize(id)),
              }
            }
          };
        }
        TableIndex::Alias(alias) => {
          let alias_name = block.store.strings.get(alias).unwrap();
          arg=format!("{}{{{},",arg,alias_name);
        },
      }
      match out_column {
        TableIndex::None => arg=format!("{}-}}",arg),
        TableIndex::All => arg=format!("{}:}}",arg),
        TableIndex::Index(ix) => arg=format!("{}{}}}",arg,ix),
        TableIndex::Table(table) => {
          match table {
            TableId::Global(id) => arg=format!("{}#{}",arg,block.store.strings.get(id).unwrap()),
            TableId::Local(id) => {
              match block.store.strings.get(id) {
                Some(name) => arg = format!("{}{}",arg,name),
                None => arg = format!("{}{}",arg,humanize(id)),
              }
            }
          };
        }
        TableIndex::Alias(alias) => {
          let alias_name = block.store.strings.get(alias).unwrap();
          arg=format!("{}.{}}}",arg,alias_name);
        },
      }
      arg
    },
    x => format!("{:?}", x),
  }
}