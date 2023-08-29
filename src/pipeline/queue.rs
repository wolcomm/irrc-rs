use std::{cmp::min, collections::VecDeque};

use crate::{error::Error, query::Query};

#[derive(Debug)]
pub(crate) struct Queue {
    q: VecDeque<Query>,
    in_flight: usize,
    max_in_flight: usize,
    min_batch: usize,
}

impl Default for Queue {
    fn default() -> Self {
        Self {
            q: VecDeque::default(),
            in_flight: 0,
            max_in_flight: 1000,
            min_batch: 100,
        }
    }
}

impl Queue {
    fn len(&self) -> usize {
        self.q.len()
    }

    pub(crate) fn push(&mut self, query: Query) {
        self.q.push_back(query);
    }

    pub(crate) fn flush<F>(&mut self, mut f: F) -> Result<(), Error>
    where
        F: FnMut(&Query) -> Result<(), Error>,
    {
        log::trace!("{} of {} queries in-flight", self.in_flight, self.len());
        if self.in_flight == self.len() {
            return Ok(());
        }
        let capacity = self.max_in_flight - self.in_flight;
        log::trace!("available capacity to flush {capacity} queries");
        if capacity >= self.min_batch {
            let upto = min(self.in_flight + capacity, self.len());
            log::debug!("trying to flush {} queries", upto - self.in_flight);
            self.q.range(self.in_flight..upto).try_for_each(|item| {
                f(item)?;
                self.in_flight += 1;
                Ok(())
            })
        } else {
            log::trace!("waiting for enough capacity to flush minimum query batch");
            Ok(())
        }
    }

    pub(crate) fn pop(&mut self) -> Option<Query> {
        if self.in_flight > 0 {
            // OK to unwrap here, as self.in_flight <= self.len()
            let item = self.q.pop_front().unwrap();
            self.in_flight -= 1;
            Some(item)
        } else {
            None
        }
    }
}
