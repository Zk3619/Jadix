use jadix_shell::{schema::*, *};
use std::collections::BTreeMap;

fn object_schema() -> Schema {
    Schema::Object {
        fields: BTreeMap::from([
            (
                "name".into(),
                Field {
                    schema: Schema::String { max_bytes: 8 },
                    required: true,
                },
            ),
            (
                "enabled".into(),
                Field {
                    schema: Schema::Bool,
                    required: false,
                },
            ),
        ]),
    }
}

fn session() -> Session {
    let mut session = Session::new(2);
    session
        .verify(PackageManifest {
            package: "schema-panel".into(),
            protocol_version: presentation::PROTOCOL_VERSION,
            requested_permissions: ["save".into()].into(),
        })
        .unwrap();
    session.authorize(&["save".into()].into()).unwrap();
    session.load(|| Ok(())).unwrap();
    session.activate().unwrap();
    session
}

#[test]
fn host_contract_validates_before_admission_and_before_result_consumption() {
    let mut registry = HostRegistry::default();
    registry
        .register_contract(
            "save".into(),
            MethodContract {
                permission: "save".into(),
                arguments: object_schema(),
                result: Schema::Bool,
            },
        )
        .unwrap();
    let mut session = session();
    assert_eq!(
        session.request(&registry, "save"),
        Err(ShellError::InvalidArguments)
    );
    assert_eq!(session.pending_requests(), 0);
    let arguments = Value::Object([("name".into(), Value::String("hello".into()))].into());
    let request = session.request_with(&registry, "save", &arguments).unwrap();
    assert_eq!(
        session.complete_with(request, &Value::Number(1.0)),
        Err(ShellError::InvalidResult)
    );
    assert_eq!(session.pending_requests(), 1);
    session.complete_with(request, &Value::Bool(true)).unwrap();
    assert_eq!(
        session.complete_with(request, &Value::Bool(true)),
        Err(ShellError::StaleRequest)
    );
    let cancelled = session.request_with(&registry, "save", &arguments).unwrap();
    session.cancel(cancelled).unwrap();
    assert_eq!(
        session.complete_with(cancelled, &Value::Bool(true)),
        Err(ShellError::StaleRequest)
    );
}

#[test]
fn schema_rejects_unknown_fields_missing_fields_and_nonfinite_numbers() {
    let schema = object_schema();
    assert!(!schema.accepts(&Value::Object(BTreeMap::new())));
    assert!(!schema.accepts(&Value::Object(
        [("name".into(), Value::String("123456789".into()))].into()
    )));
    assert!(!schema.accepts(&Value::Object(
        [
            ("name".into(), Value::String("ok".into())),
            ("extra".into(), Value::Null)
        ]
        .into()
    )));
    assert!(schema.accepts(&Value::Object(
        [
            ("name".into(), Value::String("ok".into())),
            ("enabled".into(), Value::Bool(false))
        ]
        .into()
    )));
    for number in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        assert!(!Schema::Number.accepts(&Value::Number(number)));
    }
    assert!(Schema::Number.accepts(&Value::Number(0.0)));
}

#[test]
fn arrays_utf8_payloads_depth_and_aggregate_bytes_are_bounded() {
    let schema = Schema::Array {
        items: Box::new(Schema::String { max_bytes: 4 }),
        max_items: 2,
    };
    assert!(schema.accepts(&Value::Array(vec![Value::String("\u{1f600}".into())])));
    assert!(!schema.accepts(&Value::Array(vec![Value::String("\u{1f600}a".into())])));
    assert!(!schema.accepts(&Value::Array(vec![Value::String("a".into()); 3])));
    let large = Schema::Array {
        items: Box::new(Schema::String {
            max_bytes: 1024 * 1024,
        }),
        max_items: 2,
    };
    assert!(!large.accepts(&Value::Array(vec![Value::String("a".repeat(600_000)); 2])));
    let mut deep = Schema::Null;
    let mut value = Value::Null;
    for _ in 0..34 {
        deep = Schema::Array {
            items: Box::new(deep),
            max_items: 1,
        };
        value = Value::Array(vec![value]);
    }
    assert!(!deep.is_valid());
    assert!(!deep.accepts(&value));
}

#[test]
fn method_contract_registration_does_not_replace_existing_permissions_or_schema() {
    let mut registry = HostRegistry::default();
    registry
        .register_contract(
            "save".into(),
            MethodContract {
                permission: "save".into(),
                arguments: Schema::Null,
                result: Schema::Bool,
            },
        )
        .unwrap();
    assert_eq!(
        registry.register("save".into(), "different".into()),
        Err(ShellError::InvalidManifest)
    );
    let mut session = session();
    let request = session.request(&registry, "save").unwrap();
    assert_eq!(session.complete(request), Err(ShellError::InvalidResult));
    session.complete_with(request, &Value::Bool(true)).unwrap();
}
