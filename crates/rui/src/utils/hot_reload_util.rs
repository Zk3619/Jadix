use std::collections::HashMap;
use std::sync::Mutex;

static FILE_BLOCK_COUNTER: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);

/// 为热重载提供稳定的块索引（第几个 `ui!`）。
/// 相比行号，索引在增删行时更稳定。
pub fn get_and_inc_block_index(file: &str) -> usize {
    let mut guard = FILE_BLOCK_COUNTER.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    let entry = map.entry(file.to_string()).or_insert(0);
    let val = *entry;
    *entry += 1;
    val
}
