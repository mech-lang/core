// # Errors

// Defines a struct for errors and an enum which enumerates the error types

// ## Prelude

use table::{TableIndex};
use block::Transformation;

// ## The Error Struct

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Eq, Hash)]
pub struct Error { 
  pub block_id: u64,
  pub step_text: String,
  pub error_type: ErrorType,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Eq, Hash)]
pub enum ErrorType {
  MissingAttribute(TableIndex),
  IndexOutOfBounds(((u64, u64), (u64, u64))), // (target) vs (actual) index
  DuplicateAlias(u64),                        // Alias ID
  DomainMismatch(u64, u64),                   // domain IDs (target vs actual)
  UnsatisfiedTransformation(Vec<u64>),        // IDs of missing constraints
  MissingFunction(u64),                       // ID of missing function
  IncorrectFunctionArgumentType,
}
