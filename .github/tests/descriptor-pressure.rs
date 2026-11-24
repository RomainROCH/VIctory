// Included only in the fork's temporary descriptor::host::tests module during CI.
mod pressure_warning {
    use super::{DYNAMIC, HostHeap};
    use std::collections::BTreeMap;
    use std::fmt::Debug;
    use std::sync::{Arc, Mutex};
    use tracing::field::Field;
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Level, Metadata, Subscriber};

    type Fields = BTreeMap<String, String>;

    #[derive(Clone, Default)]
    struct Warnings(Arc<Mutex<Vec<Fields>>>);

    impl Subscriber for Warnings {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            *metadata.level() == Level::WARN
        }

        fn event(&self, event: &Event<'_>) {
            let mut fields = Fields::new();
            event.record(&mut |field: &Field, value: &dyn Debug| {
                fields.insert(field.name().to_owned(), format!("{value:?}"));
            });
            if fields
                .get("message")
                .is_some_and(|m| m.contains("no free dynamic descriptor slots"))
            {
                self.0.lock().unwrap().push(fields);
            }
        }

        fn new_span(&self, _: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }
        fn record(&self, _: &Id, _: &Record<'_>) {}
        fn record_follows_from(&self, _: &Id, _: &Id) {}
        fn enter(&self, _: &Id) {}
        fn exit(&self, _: &Id) {}
    }

    #[test]
    fn observes_the_pressure_warning() {
        assert!(cfg!(feature = "diagnostics"));
        assert_eq!(std::env::var("VICTORY_D3D12_WARP").as_deref(), Ok("1"));
        let case = std::env::var("VICTORY_TEST_PRESSURE").unwrap();
        let warnings = Warnings::default();
        let expected = tracing::subscriber::with_default(warnings.clone(), || {
            let heap = HostHeap::new(false, 32).expect("WARP view heap required");
            let (n, cursor, free, longest) = match case.as_str() {
                "cursor" => {
                    let first = heap.dynamic(1).unwrap();
                    assert_eq!(first, 32);
                    heap.release(first, 1);
                    (DYNAMIC, 1, DYNAMIC, DYNAMIC)
                }
                "full" => {
                    assert_eq!(heap.dynamic(DYNAMIC), Some(32));
                    (1, 0, 0, 0)
                }
                "fragmented" => {
                    let first = heap.dynamic(1).unwrap();
                    assert_eq!(heap.dynamic(1), Some(first + 1));
                    let third = heap.dynamic(1).unwrap();
                    assert_eq!(third, first + 2);
                    assert_eq!(heap.dynamic(DYNAMIC - 3), Some(first + 3));
                    heap.release(first, 1);
                    heap.release(third, 1);
                    (2, 0, 2, 1)
                }
                _ => panic!("unrecognized case {case}"),
            };
            assert_eq!(heap.dynamic(n), None);
            [
                ("n", n),
                ("capacity", DYNAMIC),
                ("cursor", cursor),
                ("free", free),
                ("largest_run", longest),
            ]
        });
        let warnings = warnings.0.lock().unwrap();
        assert_eq!(warnings.len(), 1, "exactly one structured warning");
        let actual = &warnings[0];
        println!("observed WARN for {case}: {actual:?}");
        for (name, quantity) in expected {
            assert_eq!(
                actual.get(name),
                Some(&quantity.to_string()),
                "field {name}"
            );
        }
    }
}
