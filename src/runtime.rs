// # Mech Runtime

/* 
 The Mech Runtime is the engine that drives computations in Mech. The 
 runtime is comprised of "Blocks", interconnected by "Pipes" of records.
 Blocks can interact with the database, by Scanning for records that 
 match a pattern, or by Projecting computed records into the database.
*/

// ## Prelude

use table::{Value};
use alloc::{fmt, Vec};
use database::{Interner, Change};
use hashmap_core::map::HashMap;
use indexes::Hasher;
use operations;
use operations::Function;

// ## Runtime

#[derive(Clone)]
pub struct Runtime {
  pub blocks: Vec<Block>,
  pub pipes_map: HashMap<(u64, u64), Vec<Address>>,
}

impl Runtime {

  pub fn new() -> Runtime {
    Runtime {
      blocks: Vec::new(),
      pipes_map: HashMap::new(),
    }
  }

  // Register a new block with the runtime
  pub fn register_block(&mut self, mut block: Block, store: &mut Interner) -> Vec<Change> {
    // @TODO better block ID
    block.id = self.blocks.len() + 1;
    for ((table, column), register) in &block.pipes {
      let register_id = *register as usize - 1;
      self.pipes_map.insert((*table, *column), vec![Address{block: block.id, register: *register as usize}]);
      // Put associated values on the registers if we have them in the DB already
      block.input_registers[register_id].set(&(*table, *column));
      block.ready = set_bit(block.ready, register_id);      
    }
    self.blocks.push(block.clone());
    self.run_network(store)
  } 

  pub fn process_change(&mut self, change: &Change) {
    match change {
      Change::Add{table, row, column, value} => {
        match self.pipes_map.get(&(*table, *column)) {
          Some(addresses) => {
            for address in addresses {
              let register_ix = address.register - 1;
              let block_id = address.block - 1;
              if block_id < self.blocks.len() {
                let block = &mut self.blocks[block_id];
                if register_ix < block.input_registers.len() {
                  let register = &mut block.input_registers[register_ix];
                  //register.set_row(*row as usize, value.clone());
                  block.ready = set_bit(block.ready, register_ix);
                }
              }
            }
          },
          _ => (),
        }
      },
      _ => (),
    }
  }

  pub fn run_network(&mut self, store: &mut Interner) -> Vec<Change> {
    let mut changes = Vec::new();
    for block in &mut self.blocks {
      if block.is_ready() {
        let mut block_changes = block.solve(store);
        //changes.append(&mut block_changes);
      }
    }
    changes
  }

}

impl fmt::Debug for Runtime {
  #[inline]
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "Runtime:\n").unwrap();
    write!(f, " Blocks:\n\n").unwrap();
    for ref block in &self.blocks {
      write!(f, "{:?}\n\n", block).unwrap();
    }
    Ok(())
  }
}

// ## Blocks

#[derive(Clone)]
pub struct Address {
  pub block: usize,
  pub register: usize,
}

impl fmt::Debug for Address {
  #[inline]
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "@(block: {:?}, register: {:?})", self.block, self.register)
  }
}

#[derive(Clone)]
pub struct Register {
  pub table: u64,
  pub column: u64,
}

impl Register {
  
  pub fn new() -> Register { 
    Register {
      table: 0,
      column: 0,
    }
  }

  pub fn get(&self) -> (u64, u64) {
    (self.table, self.column)
  }

  pub fn set(&mut self, index: &(u64, u64)) {
    let (table, column) = index;
    self.table = *table;
    self.column = *column;
  }

  pub fn table(&self) -> u64 {
    self.table
  }

  pub fn column(&self) -> u64 {
    self.column
  }

}

impl fmt::Debug for Register {
  #[inline]
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "[{:?}]", self.get())
  }
}


#[derive(Clone)]
pub struct Block {
  pub id: usize,
  pub ready: u64,
  pub plan: Vec<Constraint>,
  pub pipes: HashMap<(u64, u64), u64>,
  pub input_registers: Vec<Register>,
  pub intermediate_registers: Vec<Vec<Value>>,
  pub output_registers: Vec<Register>,
  pub constraints: Vec<Constraint>,
}

impl Block {
  
  pub fn new() -> Block { 
    Block {
      id: 0,
      ready: 0,
      pipes: HashMap::new(),
      plan: Vec::new(),
      input_registers: Vec::with_capacity(32),
      intermediate_registers: Vec::with_capacity(32),
      output_registers: Vec::with_capacity(32),
      constraints: Vec::with_capacity(32),
    }
  }

  pub fn add_constraint(&mut self, constraint: Constraint) {
    match constraint {
      Constraint::Scan{table, column, register} => {
        let register_id: usize = register as usize - 1;
        // Allocate registers
        while self.input_registers.len() <= register_id {
          self.input_registers.push(Register::new());
        }
        self.pipes.insert((table, column), register);
      },
      Constraint::Insert{table, column, register} => {
        let register_id: usize = register as usize - 1;
        while self.output_registers.len() <= register_id {
          self.output_registers.push(Register::new());
        }
      },
      Constraint::Function{ref operation, ..} => {
        self.intermediate_registers.push(Vec::new());
      },
      _ => (),
    }
    self.constraints.push(constraint);
  }

  pub fn is_ready(&self) -> bool {
    let input_registers_count = self.input_registers.len();
    // TODO why does the exponent have to be u32?
    if input_registers_count > 0 {
      self.ready == 2_u64.pow(input_registers_count as u32) - 1
    } else {
      false
    }
  }

  pub fn solve(&mut self, store: &mut Interner) -> Vec<Change> {
    //self.ready = 0;
    let mut output: Vec<Change> = Vec::new();
    for step in &self.plan {
      match step {
        Constraint::Function{operation, parameters, output} => {
          // Gather references to the indicated registers as a vector
          let mut columns = Vec::new();
          for register in parameters {
            let register = &self.input_registers[*register as usize - 1];
            match store.get_column(register.table, register.column as usize) {
              Some(column) => columns.push(column),
              None => (),
            }
          }
          // Pass the parameters to the appropriate function
          let op_fun = match operation {
            Function::Add => operations::math_add,
          };
          // Execute the function. This is where the magic happens! Results are placed on the
          // intermediate registers
          op_fun(&columns, &mut self.intermediate_registers[*output as usize - 1]);
          
          // Set the result on the intended register          
          /*for (result, register) in vec![result].iter().zip(output.iter()) {
            self.intermediate_registers[0].place_data(&result);              
          }*/
        },
        Constraint::Insert{table, column, register} => {
          let column_data = &self.intermediate_registers[*register as usize - 1];
          for (row_ix, cell) in column_data.iter().enumerate() {
            store.intern_change(
              &Change::Add{table: *table, row: row_ix as u64 + 1, column: *column, value: cell.clone()}
            );
            //output.push();
          }
        },
        _ => (),
      } 
    }
    output
  }

}

impl fmt::Debug for Block {
  #[inline]
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "┌────────────────────────────────────────┐\n").unwrap();
    write!(f, "│ Block #{:?}\n", self.id).unwrap();
    write!(f, "├────────────────────────────────────────┤\n").unwrap();
    write!(f, "│ Ready: {:b}\n", self.ready).unwrap();
    write!(f, "│ Input: {:?}\n", self.input_registers.len()).unwrap();
    for (ix, register) in self.input_registers.iter().enumerate() {
      write!(f, "│  {:?}. {:?}\n", ix + 1, register).unwrap();
    }
    write!(f, "│ Intermediate: {:?}\n", self.intermediate_registers.len()).unwrap();
    for (ix, register) in self.intermediate_registers.iter().enumerate() {
      write!(f, "│  {:?}. {:?}\n", ix + 1, register).unwrap();
    }
    write!(f, "│ Output: {:?}\n", self.output_registers.len()).unwrap();
    for (ix, register) in self.output_registers.iter().enumerate() {
      write!(f, "│  {:?}. {:?}\n", ix + 1, register).unwrap();
    }
    write!(f, "│ Constraints: {:?}\n", self.constraints.len()).unwrap();
    for constraint in &self.constraints {
      write!(f, "│  > {:?}\n", constraint).unwrap();
    }
    write!(f, "│ Plan: {:?}\n", self.plan.len()).unwrap();
    for (ix, step) in self.plan.iter().enumerate() {
      write!(f, "│  {:?}. {:?}\n", ix + 1, step).unwrap();
    }
    write!(f, "└────────────────────────────────────────┘\n").unwrap();
    Ok(())
  }
}

// ## Pipe

// Pipes are conduits of records between blocks.

pub struct Pipe {
  input: Address,
  output: Address,
}

// ## Constraints

// Constraints put bounds on the data available for a block to work with. For 
// example, Scan constraints could bring data into the block, and a Join 
// constraint could match elements from one table to another.

#[derive(Clone)]
pub enum Constraint {
  // A Scan monitors a supplied cell
  Scan { table: u64, column: u64, register: u64 },
  Insert {table: u64, column: u64, register: u64},
  Function {operation: operations::Function, parameters: Vec<u64>, output: u64},
}

impl fmt::Debug for Constraint {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
      match self {
        Constraint::Scan{table, column, register} => write!(f, "Scan({:#x}, {:#x}) -> {:?}", table, column, register),
        Constraint::Insert{table, column, register} => write!(f, "Insert({:#x}, {:#x}) -> {:?}", table, column, register),
        Constraint::Function{operation, parameters, output} => write!(f, "Fxn::{:?}{:?} -> {:?}", operation, parameters, output),
        _ => Ok(()),
      }
    }
}

// ## Bit helpers

// Lifted from Eve v0.4

pub fn check_bits(solved: u64, checking: u64) -> bool {
    solved & checking == checking
}

pub fn has_any_bits(solved: u64, checking: u64) -> bool {
    solved & checking != 0
}

pub fn set_bit(solved: u64, bit: usize) -> u64 {
    solved | (1 << bit)
}

pub fn clear_bit(solved: u64, bit: usize) -> u64 {
    solved & !(1 << bit)
}

pub fn check_bit(solved: u64, bit: usize) -> bool {
    solved & (1 << bit) != 0
}
