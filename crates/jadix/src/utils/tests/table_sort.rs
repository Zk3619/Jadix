use super::*;

#[test]
fn cycle_and_sort() {
    let s0 = TableSortState::default();
    let s1 = cycle_sort(&s0, "name");
    assert_eq!(s1.dir, TableSortDir::Asc);
    let s2 = cycle_sort(&s1, "name");
    assert_eq!(s2.dir, TableSortDir::Desc);
    let cell: Rc<dyn Fn(usize, usize) -> String> = Rc::new(|r, _| ["b", "a", "c"][r].to_string());
    let order = sorted_row_order(vec![0, 1, 2], Some(0), &cell, TableSortDir::Asc);
    assert_eq!(order, vec![1, 0, 2]);
}
