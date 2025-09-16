use crate::runtime::scheduler::inject;

#[test]
fn push_and_pop() {
    const N: usize = 2;

    let inject = inject::Inject::new();

    for i in 0..N {
        assert_eq!(inject.len(), i);
        let (task, _) = super::unowned(async {});
        inject.push(task);
    }

    for i in 0..N {
        assert_eq!(inject.len(), N - i);
        assert!(inject.pop().is_some());
    }

    println!("--------------");

    assert!(inject.pop().is_none());
}

#[test]
fn pop_n_len() {
    let inject = inject::Inject::new();

    for i in 0..5 {
        assert_eq!(inject.len(), i);
        let (task, _) = super::unowned(async {});
        inject.push(task);
    }

    let mut it = inject.pop_n(3);
    assert_eq!(it.size_hint(), (0, Some(3)));
    assert!(it.next().is_some());
    assert_eq!(it.size_hint(), (0, Some(2)));
    assert!(it.next().is_some());
    assert_eq!(it.size_hint(), (0, Some(1)));
    assert!(it.next().is_some());
    assert_eq!(it.size_hint(), (0, Some(0)));
    assert!(it.next().is_none());
}

#[test]
#[ignore = "not sure why that's needed"]
fn pop_n_drains_on_drop() {
    let inject = inject::Inject::new();

    for _ in 0..10 {
        inject.push(super::unowned(async {}).0);
    }
    let _ = inject.pop_n(10);

    assert_eq!(inject.len(), 0);
}
