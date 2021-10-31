// # Errors

// Defines a struct for errors and an enum which enumerates the error types

// ## Prelude

use crate::{TableIndex, ValueKind, TableId, Transformation};

// ## The Error Struct

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MechError { 
  pub block_id: u64,
  pub step_text: String,
  pub error_type: MechErrorKind,
}

type Rows = usize;
type Cols = usize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MechErrorKind {
  MissingTable(TableId),                         // TableId of missing table
  DimensionMismatch(((Rows,Cols),(Rows,Cols))),  // Argument dimensions are mismatched ((row,col),(row,col))
  MissingColumn((TableId,TableIndex)),           // The identified table is missing a needed column
  ColumnKindMismatch(Vec<ValueKind>),            // Excepted kind versus given kind
  //IndexOutOfBounds(((u64, u64), (u64, u64))),  // (target) vs (actual) index
  //DuplicateAlias(u64),                         // Alias ID
  //DomainMismatch(u64, u64),                    // domain IDs (target vs actual)
  MissingFunction(u64),                          // ID of missing function
  TransformationPending(Transformation),         // Block is unsatisfied so the transformation is not added
  //IncorrectFunctionArgumentType,
}
