//! Mostly taken from https://github.com/changeutils/diff-rs/blob/master/src/lib.rs

use std::{collections::VecDeque, io};

pub fn diff(
    text1: &str,
    text2: &str,
) -> io::Result<Vec<String>> {
    let mut processor = Processor::new();
    {
        let mut replace = diffs::Replace::new(&mut processor);
        diffs::myers::diff(&mut replace, &text1.lines().collect::<Vec<&str>>(), &text2.lines().collect::<Vec<&str>>())?;
    }
    Ok(processor.result())
}

struct Processor {
    inserted: usize,
    removed: usize,

    context: Context,
    result: Vec<String>,
}

impl Processor {
    pub fn new() -> Self {
        Self {
            inserted: 0,
            removed: 0,

            context: Context::new(),
            result: Vec::new(),
        }
    }

    pub fn result(self) -> Vec<String> {
        self.result
    }
}

struct Context {
    pub start: Option<usize>,
    pub data: VecDeque<String>,
    pub changed: bool,

    pub counter: usize,
    pub equaled: usize,
    pub removed: usize,
    pub inserted: usize,
}

impl Context {
    pub fn new() -> Self {
        Self {
            start: None,
            data: VecDeque::new(),
            changed: false,

            counter: 0,
            equaled: 0,
            removed: 0,
            inserted: 0,
        }
    }

    pub fn to_vec(&self, removed: usize, inserted: usize) -> Vec<String> {
        let mut start = if let Some(start) = self.start {
            start
        } else {
            return Vec::new();
        };
        if start == 0 {
            start = 1;
        }
        let mut data = Vec::with_capacity(self.data.len() + 1);
        if self.changed {
            data.push(format!(
                "-{},{} +{},{}",
                start,
                self.equaled + self.removed,
                start + inserted - removed,
                self.equaled + self.inserted,
            ));
            for s in self.data.iter() {
                data.push(s.to_owned());
            }
        }
        data
    }
}

impl diffs::Diff for Processor {
    type Error = io::Error;

    fn equal(&mut self, old: usize, _new: usize, len: usize) -> Result<(), Self::Error> {
        if self.context.start.is_none() {
            self.context.start = Some(old);
        }

        self.context.counter = 0;
        for i in old..old + len {
            if !self.context.changed {
                if let Some(ref mut start) = self.context.start {
                    *start += 1;
                }
                self.context.counter += 1;
            }
            if self.context.changed && self.context.counter == 0 && len > 0 {
                self.result
                    .append(&mut self.context.to_vec(self.removed, self.inserted));

                let mut context = Context::new();

                context.counter = 0;
                context.equaled = 0;
                context.start = Some(i - 1);

                self.removed += self.context.removed;
                self.inserted += self.context.inserted;
                self.context = context;
            }
        }

        Ok(())
    }

    fn delete(&mut self, old: usize, len: usize) -> Result<(), Self::Error> {
        if self.context.start.is_none() {
            self.context.start = Some(old);
        }

        self.context.changed = true;
        self.context.removed += len;

        Ok(())
    }

    fn insert(&mut self, old: usize, _new: usize, new_len: usize) -> Result<(), Self::Error> {
        if self.context.start.is_none() {
            self.context.start = Some(old);
        }

        self.context.changed = true;
        self.context.inserted += new_len;

        Ok(())
    }

    fn replace(
        &mut self,
        old: usize,
        old_len: usize,
        _new: usize,
        new_len: usize,
    ) -> Result<(), Self::Error> {
        if self.context.start.is_none() {
            self.context.start = Some(old);
        }

        self.context.changed = true;
        self.context.removed += old_len;
        self.context.inserted += new_len;

        Ok(())
    }

    fn finish(&mut self) -> Result<(), Self::Error> {
        let truncation = self.context.counter;
        if self.context.data.len() > truncation {
            let new_size = self.context.data.len() - truncation;
            self.context.equaled -= truncation;
            self.context.data.truncate(new_size);
        }
        self.result
            .append(&mut self.context.to_vec(self.removed, self.inserted));
        Ok(())
    }
}