
use std::sync::Arc;
use std::cell::RefCell;
use std::fmt;
use std::ptr;

  // New runtime
  // requirements:
  // pass all tests
  // robust units
  // all number types
  // Unicode
  // async blocks
  // parallel operators
  // rewind capability
  // pre-serialized memory layout
  // performance target: 10 million updates per 60Hz cycle
  // stack allocated tables
  // matrix library in std

use nalgebra::base::Matrix2;
use nalgebra::base::DMatrix;
use rayon::prelude::*;
use std::collections::VecDeque;

use std::thread;
use tokio::time::{sleep,Duration};
use tokio_stream;
use futures::future::join_all;
use futures::stream::futures_unordered::FuturesUnordered;
use tokio_stream::StreamExt;
use map_in_place::MapVecInPlace;

/*
      // #ball.y{iy} := #boundary.height
      set_vs(&iy, 500.0, &mut y);

      // #ball.vy{iy | iyy} := #ball.vy * -0.80
      or_vv(&iy, &iyy, &mut iy_or);
      multiply_vs(&vy, -0.8, &mut vy2);
      set_vv(&vy2, &iy_or, &mut vy);

    // Keep the balls within the boundary height
      // ix = #ball.x > #boundary.width
      greater_than_vs(&x, 500.0, &mut ix);


      // #ball.x{ix} := #boundary.width
      set_vs(&ix, 500.0, &mut x);

      // #ball.vx{ix | ixx} := #ball.vx * -0.80
      or_vv(&ix, &ixx, &mut ix_or);
      multiply_vs(&vx, -0.8, &mut vx2);
      set_vv(&ix_or, &vx2, &mut vx);
      */

fn add_vv(lhs: &Vec<f64>, rhs: &Vec<f64>, out: &mut Vec<f64>) {
  out.iter_mut().zip(lhs).zip(rhs).for_each(|((out, lhs),rhs)| *out = *lhs + *rhs);
}

fn add_vs(lhs: &Vec<f64>, rhs: f64, out: &mut Vec<f64>) {
  out.iter_mut().zip(lhs).for_each(|(out, lhs)| *out = *lhs + rhs);
}

fn or_vv(lhs: &Vec<bool>, rhs: &Vec<bool>, out: &mut Vec<bool>) {
  out.iter_mut().zip(lhs).zip(rhs).map(|((out, lhs),rhs)| *out = *lhs || *rhs);
}

fn multiply_vs(lhs: &Vec<f64>, rhs: f64, out: &mut Vec<f64>) {
  out.iter_mut().zip(lhs).for_each(|(out, lhs)| *out = *lhs * rhs);
}

fn less_than_vs(lhs: &Vec<f64>, rhs: f64, out: &mut Vec<bool>) {
  out.iter_mut().zip(lhs).for_each(|(out, lhs)| *out = *lhs < rhs);
}

fn greater_than_vs(lhs: &Vec<f64>, rhs: f64, out: &mut Vec<bool>) {
  out.iter_mut().zip(lhs).for_each(|(out, lhs)| *out = *lhs > rhs);
}

fn set_vs(ix: &Vec<bool>, x: f64, out: &mut Vec<f64>) {
  out.iter_mut().zip(ix).map(|(out,ix)| if *ix == true {
    *out = x
  });
}

fn set_vv(ix: &Vec<bool>, x: &Vec<f64>, out: &mut Vec<f64>) {
  out.iter_mut().zip(ix).zip(x).map(|((out,ix),x)| if *ix == true {
    *out = *x
  });
}

fn set_all_vv(x: &Vec<f64>, out: &mut Vec<f64>) {
  out.iter_mut().zip(x).map(|(out,x)| *out = *x);
}

pub type MechFunction = extern "C" fn(arguments: &mut Vec<Vec<f64>>);

pub struct Table {
  pub rows: usize,
  pub cols: usize,
  data: Vec<f64>,
}

impl Table {
  pub fn new(rows: usize, cols: usize) -> Table {
    let mut table = Table {
      rows,
      cols,
      data: Vec::with_capacity(rows*cols*2),
    };
    table.data.resize(rows*cols,0.0);
    table
  }

  pub fn get_linear(&self, ix: usize) -> Option<f64> {
    if ix > self.data.len() {
      None
    } else {
      Some(self.data[ix])
    }
  }

  pub fn set_linear(&mut self, ix: usize, value: f64) -> Result<(),()> {
    if ix > self.data.len() {
      Err(())
    } else {
      self.data[ix] = value;
      Ok(())
    }
  }

  pub fn get(&self, row: usize, col: usize) -> Option<f64> {
    let ix = (col * self.rows) + row;
    if ix > self.data.len() {
      None
    } else {
      Some(self.data[ix])
    }
  }

  pub fn set(&mut self, row: usize, col: usize, value: f64) -> Result<(),()> {
    let ix = (col * self.rows) + row;
    if ix > self.data.len() {
      Err(())
    } else {
      self.data[ix] = value;
      Ok(())
    }
  }

  pub fn get_col(&mut self, col: usize) -> Option<Vec<f64>> {
    if col > self.cols {
      None
    } else {
      Some(self.data[self.rows*col..self.rows*col+self.rows].into())
    }
  }

  pub fn get_col_unchecked(&mut self, col: usize) -> Vec<f64> {
    self.data[self.rows*col..self.rows*col+self.rows].into()
  }

  pub async fn set_col(&mut self, col: usize, data: &Vec<f64>) -> Result<(),()> {
    if col > self.cols || data.len() != self.rows {
      Err(())
    } else {
      let src_len = data.len();
      let dst_len = self.data.len();
      unsafe {
        let dst_ptr = self.data.as_mut_ptr().offset((col * self.rows) as isize);
        let src_ptr = data.as_ptr();
        ptr::copy_nonoverlapping(src_ptr, dst_ptr, src_len);
      }
      Ok(())
    }
  }

  pub async fn set_col_unchecked(&mut self, col: usize, data: &Vec<f64>) {
    let src_len = data.len();
    let dst_len = self.data.len();
    unsafe {
      let dst_ptr = self.data.as_mut_ptr().offset((col * self.rows) as isize);
      let src_ptr = data.as_ptr();
      ptr::copy_nonoverlapping(src_ptr, dst_ptr, src_len);
    }
  }

}

pub async fn replace(data: &Vec<f64>, dest: &mut [f64]) {
  let src_len = data.len();
  unsafe {
    let dst_ptr = dest.as_mut_ptr();
    let src_ptr = data.as_ptr();
    ptr::copy_nonoverlapping(src_ptr, dst_ptr, src_len);
  }
}

impl fmt::Debug for Table {
  #[inline]
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    for row in 0..self.rows {
      for col in 0..self.cols {
        let v = self.get(row,col).unwrap();
        write!(f,"{:?} ", v)?;
      }
      write!(f,"\n")?;
    }
    Ok(())
  }
}

#[tokio::main]
async fn main() {
  let sizes: Vec<usize> = vec![1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7].iter().map(|x| *x as usize).collect();
  
  let start_ns0 = time::precise_time_ns();
for n in sizes { //  let n = 1e6 as usize;
  let mut balls = Table::new(n,4);
  for i in 0..n {
    balls.set(i,0,i as f64);
    balls.set(i,1,i as f64);
    balls.set(i,2,3.0);
    balls.set(i,3,4.0);
  }
  let mut col = balls.get_col(3).unwrap();
  let mut total_time = VecDeque::new();

  // Table
  let mut x = vec![1.0; n];
  let mut y = vec![1.0; n];
  let mut vx = vec![1.0; n];
  let mut vy = vec![1.0; n];

  // Temp Vars
  let mut x2 = vec![0.0; n];
  let mut y2 = vec![0.0; n];
  let mut vy2 = vec![0.0; n];
  let mut iy = vec![false; n];
  let mut iyy = vec![false; n];
  let mut iy_or = vec![false; n];
  let mut ix = vec![false; n];
  let mut ixx = vec![false; n];
  let mut ix_or = vec![false; n];
  let mut vx2 = vec![0.0; n];
  
  for _ in 0..2000 {
    let start_ns = time::precise_time_ns();
    /*if n <= 10_000 {*/
    // Update the block positions on each tick of the timer
      // #ball.x := #ball.x + #ball.vx
      add_vv(&x, &vx, &mut x2);
      set_all_vv(&x2, &mut x);

      // #ball.y := #ball.y + #ball.vy
      add_vv(&y, &vy, &mut y2);
      set_all_vv(&y2, &mut y);

      // #ball.vy := #ball.vy + #gravity
      add_vs(&vy, 1.0, &mut vy2);
      set_all_vv(&vy2, &mut vy);

    // Keep the balls within the boundary height
      // iy = #ball.y > #boundary.height
      greater_than_vs(&y, 500.0, &mut iy);

      // iyy = #ball.y < 0
      less_than_vs(&y, 0.0, &mut iyy);

      // #ball.y{iy} := #boundary.height
      set_vs(&iy, 500.0, &mut y);

      // #ball.vy{iy | iyy} := #ball.vy * -0.80
      or_vv(&iy, &iyy, &mut iy_or);
      multiply_vs(&vy, -0.8, &mut vy2);
      set_vv(&iy_or, &vy2, &mut vy);

    // Keep the balls within the boundary height
      // ix = #ball.x > #boundary.width
      greater_than_vs(&x, 500.0, &mut ix);

      // ixx = #ball.x < 0
      less_than_vs(&x, 0.0, &mut ixx);

      // #ball.x{ix} := #boundary.width
      set_vs(&ix, 500.0, &mut x);

      // #ball.vx{ix | ixx} := #ball.vx * -0.80
      or_vv(&ix, &ixx, &mut ix_or);
      multiply_vs(&vx, -0.8, &mut vx2);
      set_vv(&ix_or, &vx2, &mut vx);

    //}
    let end_ns = time::precise_time_ns();
    let time = (end_ns - start_ns) as f64;
    total_time.push_back(time);
    if total_time.len() > 1000 {
      total_time.pop_front();
    }
  }
  let average_time: f64 = total_time.iter().sum::<f64>() / total_time.len() as f64; 
  println!("{:e} - {:0.2?}Hz", n, 1.0 / (average_time / 1_000_000_000.0));
  let end_ns0 = time::precise_time_ns();
  let time = (end_ns0 - start_ns0) as f64;
  //println!("{:0.4?} s", time / 1e9);
}
}

/*
n	   In Place (Hz)	 Para In Place
1E1	 4,440,497.34 	 11,363.29 
1E2	 1,129,943.50 	 6,572.16 
1E3	 220,264.32 	   4,997.36 
1E4	 19,599.20 	     3,679.61 
1E5	 2,011.86 	     2,071.62 
1E6	 104.43 	       191.42 
1E7	 9.84 	         18.64 

*/