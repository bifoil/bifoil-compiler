use super::*;

#[test]
fn spilling_keeps_live_records_through_reads_deletions_and_reinsertion() {
    let mut index = RecordIndex::new(2);
    for n in 0..50 {
        index
            .insert(
                format!("module.f{n:03}"),
                Location {
                    offset: n,
                    length: n + 100,
                    instructions: n as usize + 1,
                    checksum: [n as u8; 32],
                    header_checksum: [9; 32],
                },
            )
            .unwrap();
    }
    assert!(matches!(index.storage, Storage::Disk(_)));
    index.finish().unwrap();
    for n in 0..50 {
        let value = index.get(&format!("module.f{n:03}")).unwrap().unwrap();
        assert_eq!(value.offset, n);
        assert_eq!(value.length, n + 100);
        assert_eq!(value.instructions, n as usize + 1);
        assert_eq!(value.checksum, [n as u8; 32]);
        assert_eq!(value.header_checksum, [9; 32]);
    }
    for n in 0..50 {
        assert!(index.remove(&format!("module.f{n:03}")).unwrap().is_some());
    }
    assert!(index.is_empty());
    index.finish().unwrap();
    assert!(index.get("module.f000").unwrap().is_none());
    index
        .insert(
            "module.f000".into(),
            Location {
                offset: 500,
                length: 12,
                instructions: 2,
                checksum: [3; 32],
                header_checksum: [4; 32],
            },
        )
        .unwrap();
    assert_eq!(index.get("module.f000").unwrap().unwrap().offset, 500);
    index.finish().unwrap();
    assert_eq!(index.get("module.f000").unwrap().unwrap().offset, 500);
}

#[test]
fn bounded_read_snapshots_are_released_before_mutation() {
    let mut index = RecordIndex::new(0);
    let row = Location {
        offset: 7,
        length: 100,
        instructions: 2,
        checksum: [1; 32],
        header_checksum: [2; 32],
    };
    index.insert("module.first".into(), row.clone()).unwrap();
    index.insert("module.second".into(), row).unwrap();
    index.finish().unwrap();
    {
        let _scope = index.read_scope(["module.first"]).unwrap();
        assert!(index.snapshot.borrow().as_ref().unwrap().contains_key("module.first"));
        assert_eq!(index.get("module.first").unwrap().unwrap().offset, 7);
        assert_eq!(index.get("module.second").unwrap().unwrap().offset, 7);
        {
            let _nested = index.read_scope(["module.second"]).unwrap();
            assert_eq!(index.get("module.second").unwrap().unwrap().offset, 7);
        }
        assert!(index.snapshot.borrow().is_some());
    }
    assert!(index.snapshot.borrow().is_none());
    index.remove("module.first").unwrap();
    index.finish().unwrap();
    assert!(index.get("module.first").unwrap().is_none());
}

#[test]
fn read_snapshot_limit_falls_back_without_hiding_missing_or_damaged_rows() {
    let mut index = RecordIndex::new(0);
    let row = Location {
        offset: 7,
        length: 100,
        instructions: 2,
        checksum: [1; 32],
        header_checksum: [2; 32],
    };
    index.insert("first".into(), row.clone()).unwrap();
    index.insert("second".into(), row).unwrap();
    index.finish().unwrap();
    let limit = "first".len() + std::mem::size_of::<(String, Location)>() + 64;
    {
        let _scope = index.read_scope_with_limit(["first", "second"], limit).unwrap();
        assert_eq!(index.snapshot.borrow().as_ref().unwrap().len(), 1);
        assert_eq!(index.get("second").unwrap().unwrap().offset, 7);
        assert!(index.get("missing").unwrap().is_none());
    }
    assert!(index.read_scope(["first", "missing"]).is_err());
    assert!(index.snapshot.borrow().is_none());
    index.damage("second").unwrap();
    {
        let _scope = index.read_scope_with_limit(["first", "second"], limit).unwrap();
        assert!(index.get("second").is_err());
    }
    assert!(index.snapshot.borrow().is_none());
}

#[test]
fn abandoned_vacancies_do_not_reserve_identities_or_change_counts() {
    let mut index = RecordIndex::new(0);
    {
        let _abandoned = index.vacant("module.function".into()).unwrap();
    }
    assert!(index.is_empty());
    assert!(!index.contains_key("module.function").unwrap());
    index
        .vacant("module.function".into())
        .unwrap()
        .insert(Location {
            offset: 1,
            length: 2,
            instructions: 3,
            checksum: [4; 32],
            header_checksum: [5; 32],
        })
        .unwrap();
    assert_eq!(index.count, 1);
    assert!(index.vacant("module.function".into()).is_err());
    assert_eq!(index.count, 1);
}
