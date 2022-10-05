use std::sync::Arc;

use parking_lot::Mutex;

pub struct SharedInfo<'a> {
    diffs: Arc<Mutex<&'a mut Vec<String>>>,
    green_lines: Arc<Mutex<&'a mut Vec<String>>>,
    found_params: Arc<Mutex<&'a mut Vec<String>>>,
}