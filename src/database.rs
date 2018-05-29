// # Database

// ## Prelude

use alloc::{String, Vec};
use core::fmt;
use table::{Value, Table};
use indexes::{TableIndex, Hasher};
use hashmap_core::set::{HashSet};
use hashmap_core::map::{HashMap};
use runtime::{Runtime, Block};

// ## Changes

#[derive(Clone)]
pub enum Change {
  Add{table: u64, row: u64, column: u64, value: Value},
  Remove{table: u64, row: u64, column: u64, value: Value},
  NewTable{tag: u64, rows: usize, columns: usize},
}

impl fmt::Debug for Change {
  #[inline]
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match self {
      Change::Add{table, row, column, value} => write!(f, "+>> #{:#x} [{:#x} {:#x}: {:?}]", table, row, column, value),
      Change::Remove{table, row, column, value} => write!(f, "- #{:#x} [{:#x} {:#x}: {:?}]", table, row, column, value),
      Change::NewTable{tag, rows, columns} => write!(f, "+ #{:#x} [{:?} x {:?}]", tag, rows, columns),
    }
  }
}
  
// ## Transaction

#[derive(Clone)]
pub struct Transaction {
  pub timestamp: u64,
  complete: bool,
  pub epoch: u64,
  pub round: u64,
  pub tables: Vec<Change>,
  pub adds: Vec<Change>,
  pub removes: Vec<Change>,
}

impl Transaction {
  pub fn new() -> Transaction {
    Transaction {
      timestamp: 0,
      complete: false,
      epoch: 0,
      round: 0,
      tables: Vec::new(),
      adds: Vec::new(),
      removes: Vec::new(),
    }
  }

  pub fn from_changeset(changes: Vec<Change>) -> Transaction {
    let mut txn = Transaction::new();
    for change in changes {
      match change {
        Change::Add{..} => txn.adds.push(change),
        Change::Remove{..} => txn.removes.push(change),
        Change::NewTable{..} => txn.tables.push(change),
      }
    }
    txn
  }

  pub fn from_change(change: Change) -> Transaction {
    let mut txn = Transaction::new();
    match change {
      Change::Add{..} => txn.adds.push(change),
      Change::Remove{..} => txn.removes.push(change),
      Change::NewTable{..} => txn.tables.push(change),
    }
    txn
  }

  pub fn from_adds_removes(adds: Vec<(u64, u64, u64, u64)>, removes: Vec<(u64, u64, u64, u64)>) -> Transaction {
    let mut txn = Transaction::new();
    for (table, row,column, value) in adds {
      txn.adds.push(Change::Add{table, row, column, value: Value::from_u64(value)});
    }
    for (table, row,column, value) in removes {
      txn.adds.push(Change::Remove{table, row, column, value: Value::from_u64(value)});
    }
    txn    
  }

  pub fn is_complete(&self) -> bool {
    self.complete == true
  }
}

impl fmt::Debug for Transaction {
  #[inline]
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    for ref table in &self.tables {
      write!(f, "{:?}\n", table).unwrap();
    }
    for ref add in &self.adds {
      write!(f, "{:?}\n", add).unwrap();
    }
    for ref remove in &self.removes {
      write!(f, "{:?}\n", remove).unwrap();
    }
    Ok(())
  }
}

// ## Interner

#[derive(Debug)]
pub struct Interner {
  pub tables: TableIndex,
  pub changes: Vec<Change>,
  changes_count: usize,
  pub change_pointer: usize,
  rollover: usize,
}

impl Interner {

  pub fn new(change_capacity: usize, table_capacity: usize) -> Interner {
    Interner {
      tables: TableIndex::new(table_capacity),
      changes: Vec::with_capacity(change_capacity),
      changes_count: 0,
      change_pointer: 0,
      rollover: 0,
    }
  }

  pub fn intern_change(&mut self, change: &Change) {
    match change {
      Change::Add{table, row, column, value} => {
        match self.tables.get_mut(*table) {
          Some(table_ref) => {
            table_ref.grow_to_fit(*row as usize, *column as usize);
            table_ref.set_cell(*row as usize, *column as usize, value.clone());
          }
          None => (),
        };
        self.tables.changed.insert(*table);
      },
      // TODO Implement removes
      Change::Remove{..} => {

      }
      Change::NewTable{tag, rows, columns } => {
        if !self.tables.name_map.contains_key(&tag) {
          self.tables.name_map.insert(*tag, 0);
          self.tables.register(Table::new(*tag, *rows, *columns));
        }
      }
    }
    // Intern the change. If there's enough room in memory, store it there. 
    // If not, make room by evicting some old change and throw that on disk. 
    // For now, we'll make the policy that the oldest record get evicted first.
    if self.changes.len() < self.changes.capacity() {
      self.changes.push(change.clone());
    } else if self.change_pointer == self.changes.capacity() {
      self.change_pointer = 0;
      self.rollover += 1;
      self.changes[self.change_pointer] = change.clone();
    } else {
      self.changes[self.change_pointer] = change.clone();
    }
    self.change_pointer += 1;
    self.changes_count += 1;
  }

  pub fn get_column(&self, table: u64, column_ix: usize) -> Option<&Vec<Value>> {
    match self.tables.get(table) {
      Some(stored_table) => {
        match stored_table.get_column(column_ix) {
          Some(column) => Some(column),
          None => None,
        }
      },
      None => None,
    }
  }

  pub fn get_cell(&self, table: u64, row_ix: usize, column_ix: usize) -> Option<&Value> {
    match self.tables.get(table) {
      Some(stored_table) => {
        stored_table.index(row_ix, column_ix)
      },
      None => None,
    }
  }

  pub fn len(&self) -> usize {
    self.changes_count as usize
  }

}

// ## Database

pub struct Database {
  pub epoch: u64,
  pub round: u64,
  pub processed: usize,
  pub store: Interner,
  pub runtime: Runtime,
  pub watched_index: HashMap<u64, bool>,
  pub last_transaction: usize,
}

impl Database {

  pub fn new(change_capacity: usize, table_capacity: usize) -> Database {
    Database {
      epoch: 0,
      round: 0,
      processed: 0,
      store: Interner::new(change_capacity, table_capacity),
      runtime: Runtime::new(),
      watched_index: HashMap::new(),
      last_transaction: 0,
    }
  }

  pub fn register_watcher(&mut self, table: u64) {
    self.watched_index.insert(table, false);
  }

  pub fn process_transaction(&mut self, txn: &Transaction) {
    self.last_transaction = self.store.change_pointer;
    // First make any tables
    for table in txn.tables.iter() {
      self.store.intern_change(table);
    }
    // Handle the removes
    for remove in txn.removes.iter() {
      self.store.intern_change(remove);
    }
    // Handle the adds
    for add in txn.adds.iter() {
      self.store.intern_change(add);
    }
    self.runtime.run_network(&mut self.store);
    // Mark watched tables as changed
    for changed_tables in self.store.tables.changed.drain() {
      match self.watched_index.get_mut(&changed_tables) {
        Some(q) => *q = true,
        _ => (),
      }
    }
    self.epoch = self.store.rollover as u64;
  }

  pub fn capacity(&self) -> f64 {
    100.0 * (self.store.changes.len() as f64 / self.store.changes.capacity() as f64)
  }
}

impl fmt::Debug for Database {
  #[inline]
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "┌────────────────────┐\n").unwrap();
    write!(f, "│ Database ({:?})\n", self.store.changes.capacity()).unwrap();
    write!(f, "├────────────────────┤\n").unwrap();
    write!(f, "│ Epoch: {:?}\n", self.epoch).unwrap();
    write!(f, "│ Changes: {:?}\n", self.store.len()).unwrap();
    write!(f, "│ Capacity: {:0.2}%\n", 100.0 * (self.store.changes.len() as f64 / self.store.changes.capacity() as f64)).unwrap();
    write!(f, "│ Tables: {:?}\n", self.store.tables.len()).unwrap();
    write!(f, "│ Blocks: {:?}\n", self.runtime.blocks.len()).unwrap();
    write!(f, "└────────────────────┘\n").unwrap();
    for (table, history) in self.store.tables.map.values() {
      write!(f, "{:?}", table).unwrap();
    }
    Ok(())
  }
}