#![feature(get_mut_unchecked)]
#![feature(concat_idents)]
#![allow(warnings)]
#![feature(iter_intersperse)]
#![feature(drain_filter)]

extern crate core as rust_core;
extern crate hashbrown;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate num_traits;

#[macro_use]
extern crate lazy_static;
extern crate seahash;
extern crate indexmap;
use std::rc::Rc;
use std::cell::RefCell;
use std::fmt;
use num_traits::*;
use std::ops::*;


mod column;
mod value;
mod error;
mod table;
mod transformation;
mod database;
mod user_functions;
#[cfg(feature = "stdlib")]
pub mod function;
mod block;
mod core;
mod schedule;
pub mod nodes;



pub use self::core::Core;
pub use self::table::*;
pub use self::column::*;
pub use self::value::*;
pub use self::error::*;
pub use self::transformation::Transformation;
pub use self::database::*;
#[cfg(feature = "stdlib")]
pub use self::function::*;
pub use self::block::*;
pub use self::schedule::*;
pub use self::user_functions::*;


pub type BlockId = u64;
pub type ArgumentName = u64;
pub type Argument = (ArgumentName, TableId, Vec<(TableIndex, TableIndex)>);
pub type Out = (TableId, TableIndex, TableIndex);


pub type Arg<T> = ColumnV<T>;
pub type ArgTable = Rc<RefCell<Table>>;
pub type OutTable = Rc<RefCell<Table>>;

pub trait MechNumArithmetic<T>: Add<Output = T> + 
                                Sub<Output = T> + 
                                Div<Output = T> + 
                                Mul<Output = T> + 
                                Pow<T, Output = T> + 
                                AddAssign +
                                SubAssign +
                                MulAssign +
                                DivAssign +
                                Sized {}

#[derive(Debug)]
pub enum SectionElement {
  Block(Block),
  UserFunction(UserFunction),
}

pub trait MechFunctionCompiler {
  fn compile(&self, block: &mut Block, arguments: &Vec<Argument>, out: &Out) -> std::result::Result<(),MechError>;
}

pub trait MechFunction {
  fn solve(&self);
  fn to_string(&self) -> String;
}

pub fn resize_one(block: &mut Block, out: &Out) -> std::result::Result<(),MechError> {
  let (out_table_id,_,_) = out;
  let out_table = block.get_table(out_table_id)?;
  let mut out_brrw = out_table.borrow_mut();
  out_brrw.resize(1,1);
  Ok(())
}

pub trait Machine {
  fn name(&self) -> String;
  fn id(&self) -> u64;
  fn on_change(&mut self, table: &Table) -> Result<(), MechError>;
}


#[derive(Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct MechString {
  chars: Vec<char>,
}

impl MechString {

  pub fn new() -> MechString {
    MechString {
      chars: vec![],
    }
  }

  pub fn from_string(string: String) -> MechString {
    MechString {
      chars: string.chars().collect::<Vec<char>>()
    }
  }

  pub fn from_str(string: &str) -> MechString {
    MechString {
      chars: string.chars().collect::<Vec<char>>()
    }
  }

  pub fn from_chars(chars: &Vec<char>) -> MechString {
    MechString {
      chars: chars.clone(),
    }
  }

  pub fn len(&self) -> usize {
    self.chars.iter().count()
  }

  pub fn to_string(&self) -> String {
    self.chars.iter().collect::<String>()
  }

  pub fn hash(&self) -> u64 {
    hash_chars(&self.chars)
  }
}

impl fmt::Debug for MechString {
  #[inline]
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f,"{}",self.to_string())?;
    Ok(())
  }
}

pub fn hash_chars(input: &Vec<char>) -> u64 {
  seahash::hash(input.iter().map(|s| String::from(*s)).collect::<String>().as_bytes()) & 0x00FFFFFFFFFFFFFF
}

pub fn hash_bytes(input: &Vec<u8>) -> u64 {
  seahash::hash(input) & 0x00FFFFFFFFFFFFFF
}

pub fn hash_str(input: &str) -> u64 {
  seahash::hash(input.to_string().as_bytes()) & 0x00FFFFFFFFFFFFFF
}

pub fn humanize(hash: &u64) -> String {
  let bytes: [u8; 8] = hash.to_be_bytes();
  let mut string = "".to_string();
  let mut ix = 0;
  for byte in bytes.iter() {
    if ix % 2 == 0 {
      ix += 1;
      continue;
    }
    string.push_str(&WORDLIST[*byte as usize]);
    if ix < 7 {
      string.push_str("-");
    }
    ix += 1;
  }
  string
}


pub const WORDLIST: &[&str;256] = &[
  "nil", "ama", "ine", "ska", "pha", "gel", "art", 
  "ona", "sas", "ist", "aus", "pen", "ust", "umn",
  "ado", "con", "loo", "man", "eer", "lin", "ium",
  "ack", "som", "lue", "ird", "avo", "dog", "ger",
  "ter", "nia", "bon", "nal", "ina", "pet", "cat",
  "ing", "lie", "ken", "fee", "ola", "old", "rad",
  "met", "cut", "azy", "cup", "ota", "dec", "del",
  "elt", "iet", "don", "ble", "ear", "rth", "eas", 
  "war", "eig", "tee", "ele", "emm", "ene", "qua",
  "tst", "fan", "fif", "fil", "fin", "fis", "fiv", 
  "flo", "for", "foo", "fou", "fot", "fox", "fre",
  "fri", "fru", "gee", "gia", "glu", "fol", "gre", 
  "ham", "hap", "har", "haw", "hel", "hig", "hot", 
  "hyd", "ida", "ill", "ind", "ini", "ink", "iwa",
  "and", "ite", "jer", "jig", "joh", "jul", "uly", 
  "kan", "ket", "kil", "kin", "kit", "lac", "lak", 
  "lem", "ard", "lim", "lio", "lit", "lon", "lou",
  "low", "mag", "nes", "mai", "gam", "arc", "mar",
  "mao", "mas", "may", "mex", "mic", "mik", "ril",
  "min", "mir", "mis", "mio", "mob", "moc", "ech",
  "moe", "tan", "oon", "ain", "mup", "sic", "neb",
  "une", "net", "nev", "nin", "een", "nit", "nor",
  "nov", "nut", "oct", "ohi", "okl", "one", "ora",
  "ges", "ore", "osc", "ove", "oxy", "pap", "par", 
  "pey", "pip", "piz", "plu", "pot", "pri", "pur",
  "que", "uqi", "qui", "red", "riv", "rob", "roi", 
  "rug", "sad", "sal", "sat", "sep", "sev", "eve",
  "sha", "sie", "sin", "sik", "six", "sit", "sky", 
  "soc", "sod", "sol", "sot", "tir", "ker", "spr",
  "sta", "ste", "mam", "mer", "swe", "tab", "tag", 
  "see", "nis", "tex", "thi", "the", "tim", "tri",
  "twe", "ent", "two", "unc", "ess", "uni", "ura", 
  "veg", "ven", "ver", "vic", "vid", "vio", "vir",
  "was", "est", "whi", "hit", "iam", "win", "his",
  "wis", "olf", "wyo", "ray", "ank", "yel", "zeb",
  "ulu", "fix", "gry", "hol", "jup", "lam", "pas",
  "rom", "sne", "ten", "uta"];


#[derive(Debug)]
pub enum LineKind {
  Title((String,String)),
  String(String),
  Table(BoxTable),
  Separator,
}


#[derive(Debug)]
pub struct BoxTable {
  pub title: String,
  pub rows: usize,
  pub cols: usize,
  pub column_aliases: Vec<String>,
  pub column_kinds: Vec<String>,
  pub strings: Vec<Vec<String>>,
  pub width: usize,
  pub column_widths: Vec<usize>,
}

impl BoxTable {

  pub fn new(table: &Table) -> BoxTable {
    let table_name: String = if let Some(mstring) = table.dictionary.borrow().get(&table.id) {
      format!("#{}", mstring.to_string())
    } else {
      format!("{}", humanize(&table.id))
    };
    let dynamic = if table.dynamic {
      "+"
    } else {
      ""
    };
    let title = format!("{}{} ({} x {})", table_name,dynamic,table.rows,table.cols);
    let mut strings: Vec<Vec<String>> = vec![vec!["".to_string(); table.rows]; table.cols];
    let mut column_widths = vec![0; table.cols];
    let mut column_aliases = Vec::new();
    let mut column_kinds = Vec::new();
    for (col,(alias,ix)) in table.col_map.iter().enumerate() {
      if let Some(alias_string) = table.dictionary.borrow().get(alias) {
        let chars = alias_string.len();
        if chars > column_widths[col] {
          column_widths[col] = chars;
        }
        let alias = format!("{}", alias_string.to_string());
        column_aliases.push(alias);   
      } else {
        let alias = format!("{}", humanize(alias));
        let chars = alias.len();
        if chars > column_widths[col] {
          column_widths[col] = chars;
        }
        column_aliases.push(alias);   
      }
    }

    for (col,kind) in table.col_kinds.iter().enumerate() {
      let kind_string = format!("{:?}", kind);
      let chars = kind_string.len();
      if chars > column_widths[col] {
        column_widths[col] = chars;
      }
      column_kinds.push(kind_string);   
    }

    for row in 0..table.rows {
      for col in 0..table.cols {
        let value_string = match table.get_raw(row,col) {
          Ok(v) => format!("{:?}", v), 
          Err(x) => format!("{:?}",x),
        };
        let chars = value_string.chars().count();
        if chars > column_widths[col] {
          column_widths[col] = chars;
        }
        strings[col][row] = value_string;
      }
    }
    let width = column_widths.iter().sum();
    if width == 0 {column_widths.push(0);}
    BoxTable {
      title,
      width,
      rows: table.rows,
      cols: table.cols,
      column_aliases,
      column_kinds,
      strings,
      column_widths,
    }
  }

}


pub struct BoxPrinter {
  pub lines: Vec<LineKind>,
  width: usize,
  drawing: String,
}

impl BoxPrinter {

  pub fn new() -> BoxPrinter {
    BoxPrinter {
      lines: Vec::new(),
      width: 30,
      drawing: "\n┌─┐\n│ │\n└─┘\n".to_string(),
    }
  }

  pub fn add_line(&mut self, lines: String) {
    for line in lines.lines() {
      let chars = line.chars().count();
      if chars > self.width {
        self.width = chars;
      }
      self.lines.push(LineKind::String(line.to_string()));
    }
    self.render_box();
  }

  pub fn add_title(&mut self, icon: &str, lines: &str) {
    self.add_separator();
    for line in lines.lines() {
      let chars = line.chars().count() + 3;
      if chars > self.width {
        self.width = chars;
      }
      self.lines.push(LineKind::Title((icon.to_string(),line.to_string())));
    }
    self.render_box();
    self.add_separator();
  }

  pub fn add_header(&mut self, text: &str) {
    self.add_separator();
    self.add_line(text.to_string());
    self.add_separator();
  }

  pub fn add_separator(&mut self) {
    if self.lines.len() > 0 {
      self.lines.push(LineKind::Separator);
      self.render_box();
    }
  }

  pub fn add_table(&mut self, table: &Table) {
    let bt = BoxTable::new(table);
    self.width = if bt.width + bt.cols > self.width {
      bt.width + bt.cols - 1
    } else {
      self.width
    };
    self.lines.push(LineKind::Table(bt));
    self.render_box();
  }

  fn render_box(&mut self) {
    let top = "\n╭".to_string() + &BoxPrinter::format_repeated_char("─", self.width) + &"╮\n".to_string();
    let mut middle = "".to_string();
    let mut bottom = "╰".to_string() + &BoxPrinter::format_repeated_char("─", self.width) + &"╯\n".to_string();
    
    for line in &self.lines {

      match line {
        LineKind::Separator => {
          let boxed_line = "├".to_string() + &BoxPrinter::format_repeated_char("─", self.width) + &"┤\n".to_string();
          middle += &boxed_line;
        }
        LineKind::Table(table) => {
          let mut column_widths = table.column_widths.clone();
          if table.width + table.cols < self.width {
            let mut diff = self.width - (table.width + table.cols) + 1;
            let mut ix = 0;
            while diff > 0 {
              let c = column_widths.len();
              column_widths[ix % c] += 1;
              ix += 1;
              diff -= 1; 
            }
          }
          if self.width < table.title.chars().count() {
            self.width = table.title.chars().count() + 10;
            let col_width = self.width / table.cols;
            for (ix,mut w) in column_widths.iter_mut().enumerate() {
              *w = col_width;
            }
          }
          // Print table header
          middle += "│";
          middle += &table.title;
          middle += &BoxPrinter::format_repeated_char(" ", self.width - table.title.chars().count());
          middle += "│\n";
          if table.column_aliases.len() > 0 {
            middle += "├";
            for col in 0..table.cols-1 {
              middle += &BoxPrinter::format_repeated_char("─", column_widths[col]);
              middle += "┬";
            }
            middle += &BoxPrinter::format_repeated_char("─", *column_widths.last().unwrap());
            middle += "┤\n";
            let mut boxed_line = "│".to_string();
            for col in 0..table.cols {
              let cell = &table.column_aliases[col];
              let chars = cell.chars().count();
              boxed_line += &cell; 
              boxed_line += &BoxPrinter::format_repeated_char(" ", column_widths[col] - chars);
              boxed_line += "│";
            }
            boxed_line += &"\n".to_string();
            middle += &boxed_line;
          }
          if table.cols > 0 {
            if table.column_kinds.len() > 0 {
              middle += "├";
              for col in 0..table.cols-1 {
                middle += &BoxPrinter::format_repeated_char("─", column_widths[col]);
                middle += "┼";
              }
              middle += &BoxPrinter::format_repeated_char("─", *column_widths.last().unwrap());
              middle += "┤\n";
              let mut boxed_line = "│".to_string();
              for col in 0..table.cols {
                let cell = &table.column_kinds[col];
                let chars = cell.chars().count();
                boxed_line += &cell; 
                boxed_line += &BoxPrinter::format_repeated_char(" ", column_widths[col] - chars);
                boxed_line += "│";
              }
              boxed_line += &"\n".to_string();
              middle += &boxed_line;
            }
            middle += "├";
            for col in 0..table.cols-1 {
              middle += &BoxPrinter::format_repeated_char("─", column_widths[col]);
              middle += "┼";
            }
            middle += &BoxPrinter::format_repeated_char("─", *column_widths.last().unwrap());
            middle += "┤\n";
          }
          if table.cols == 0 {
            continue;
          }
          // Print at most 10 rows
          for row in (0..table.rows).take(10) {
            let mut boxed_line = "│".to_string();
            for col in 0..table.cols {
              let cell = &table.strings[col][row];
              let chars = cell.chars().count();
              boxed_line += &cell; 
              boxed_line += &BoxPrinter::format_repeated_char(" ", column_widths[col] - chars);
              boxed_line += "│";
            }
            boxed_line += &"\n".to_string();
            middle += &boxed_line;
          }
          if table.rows > 10 {
            // Print ...
            if table.rows > 11 {
              let mut boxed_line = "│".to_string();
              for col in 0..table.cols {
                boxed_line += "..."; 
                boxed_line += &BoxPrinter::format_repeated_char(" ", column_widths[col] - 3);
                boxed_line += "│";
              }
              boxed_line += &"\n".to_string();
              middle += &boxed_line;
            }
            // Print last row
            let mut boxed_line = "│".to_string();
            for col in 0..table.cols {
              let cell = &table.strings[col][table.rows - 1];
              let chars = cell.chars().count();
              boxed_line += &cell; 
              boxed_line += &BoxPrinter::format_repeated_char(" ", column_widths[col] - chars);
              boxed_line += "│";
            }
            boxed_line += &"\n".to_string();
            middle += &boxed_line;
          }
          bottom = "╰".to_string(); 
          for col in 0..table.cols-1 {
            bottom += &BoxPrinter::format_repeated_char("─", column_widths[col]);
            bottom += &"┴".to_string();
          }
          bottom += &BoxPrinter::format_repeated_char("─", *column_widths.last().unwrap());
          bottom += &"╯\n".to_string();
        }
        LineKind::String(line) => {
          let chars = line.chars().count();
          if self.width >= chars {
            let boxed_line = "│".to_string() + &line + &BoxPrinter::format_repeated_char(" ", self.width - chars) + &"│\n".to_string();
            middle += &boxed_line;
          } else {
            println!("Line too long: {:?}", line);
          }
        }
        LineKind::Title((icon,line)) => {
          let chars = line.chars().count() + 3;
          if self.width >= chars {
            let boxed_line = "│".to_string() + &icon + " " + &line + &BoxPrinter::format_repeated_char(" ", self.width - chars) + &"│\n".to_string();
            middle += &boxed_line;
          } else {
            println!("Line too long: {:?}", line);
          }
        }
      }
    }
    self.drawing = top + &middle + &bottom;
  }

  pub fn print(&self) -> String {
    self.drawing.clone()
  }

  fn format_repeated_char(to_print: &str, n: usize) -> String {
    let mut s = "".to_string();
    for _ in 0..n {
      s = format!("{}{}",s,to_print);
    }
    s
  }

}

impl fmt::Debug for BoxPrinter {
  #[inline]
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f,"{}",self.drawing)?;
    Ok(())
  }
}
