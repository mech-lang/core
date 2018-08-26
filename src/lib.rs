// # Mech

/*
Mech is a programming language especially suited for developing reactive 
systems. 
*/

// ## Prelude

#![cfg_attr(target_os = "none", no_std)]
#![feature(alloc)]
#![feature(nll)]

extern crate rlibc;
#[macro_use]
extern crate alloc;
#[cfg(not(target_os = "none"))]
extern crate core;
extern crate hashmap_core;
extern crate rand;
#[macro_use]
extern crate serde_derive;

use alloc::{String, Vec};
use core::fmt;
use hashmap_core::set::{HashSet};
use hashmap_core::map::{HashMap};

// ## Modules

mod database;
mod runtime;
mod table;
mod indexes;
mod operations;

// ## Exported Modules

pub use self::database::{Transaction, Change, Interner};
pub use self::table::{Value, Table};
pub use self::indexes::{TableIndex, Hasher};
pub use self::operations::{Function, Plan, Comparator};
pub use self::runtime::{Runtime, Block, Constraint, Register};

// ## Core

pub struct Core {
  pub id: u64,
  pub epoch: usize,
  pub time: usize, // this is an offset from now. 0 means now, 1 means 1 tick ago, etc.
  pub round: usize,
  pub changes: usize,
  pub store: Interner,
  pub runtime: Runtime,
  pub watched_index: HashMap<u64, bool>,
  change_capacity: usize,
  table_capacity: usize,
  transaction_boundaries: Vec<usize>,
}

impl Core {

  pub fn new(change_capacity: usize, table_capacity: usize) -> Core {
    Core {
      id: 0,
      time: 0,
      epoch: 0,
      round: 0,
      changes: 0,
      change_capacity,
      table_capacity,
      store: Interner::new(change_capacity, table_capacity),
      runtime: Runtime::new(),
      watched_index: HashMap::new(),
      transaction_boundaries: Vec::new(),
    }
  }

  pub fn clear(&mut self) {
    self.epoch = 0;
    self.round = 0;
    self.runtime.clear();
    self.store.clear();
    self.watched_index.clear();
  }

  pub fn register_blocks(&mut self, blocks: Vec<Block>) {
    self.runtime.register_blocks(blocks, &mut self.store);
  }

  pub fn register_watcher(&mut self, table: u64) {
    self.watched_index.insert(table, false);
  }

  pub fn step(&mut self) {
    self.runtime.run_network(&mut self.store);
  }

  pub fn index(&mut self, table: u64, row: u64, column: u64) -> Option<&Value> {
    match self.store.tables.get(table) {
      Some(table_ref) => {
        match table_ref.index(row as usize, column as usize) {
          Some(cell_data) => Some(cell_data),
          None => None,
        }
      },
      None => None,
    }
  }

  pub fn index_by_alias(&mut self, table: u64, row: u64, column: &u64) -> Option<&Value> {
    match self.store.tables.get(table) {
      Some(table_ref) => {
        match table_ref.index_by_alias(row as usize, column) {
          Some(cell_data) => Some(cell_data),
          None => None,
        }
      },
      None => None,
    }
  }

  pub fn step_backward(&mut self, steps: usize) {

  }

  pub fn step_back_one(&mut self) {
    let time = self.time;
    let transactions = self.transaction_boundaries.len();
    // We can only step back if there is at least one transaction, 
    // and we aren't at the beginning of time
    if transactions > 0  {
      let now_ix = if time == 0 {
        self.store.change_pointer
      } else if transactions == time {
        0
      } else {
        self.transaction_boundaries[transactions - time]
      };
      println!("{:?}", now_ix);
    }

    for change in &self.store.changes {
      println!("{:?}",change);
    }
    println!("{:?}",self.transaction_boundaries);
  }

  pub fn process_transaction(&mut self, txn: &Transaction) {

    self.transaction_boundaries.push(self.store.change_pointer);
    self.store.process_transaction(txn);
    self.runtime.run_network(&mut self.store);
    
    // Mark watched tables as changed
    for (table_id, _) in self.store.tables.changed.iter() {
      match self.watched_index.get_mut(&(*table_id as u64)) {
        Some(q) => *q = true,
        _ => (),
      }
    }

    self.changes = self.store.changes_count;
    self.epoch = self.store.rollover;
  }

  pub fn capacity(&self) -> f64 {
    100.0 * (self.store.changes.len() as f64 / self.store.changes.capacity() as f64)
  }
}

impl fmt::Debug for Core {
  #[inline]
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "┌────────────────────┐\n").unwrap();
    write!(f, "│ Mech Core #{:0x}\n", self.id).unwrap();
    write!(f, "├────────────────────┤\n").unwrap();
    write!(f, "│ Epoch: {:?}\n", self.epoch).unwrap();
    write!(f, "│ Changes: {:?}\n", self.changes).unwrap();
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