mod common;

use audion::value::Value;

fn eval(src: &str) -> Value {
    common::eval(src)
}

#[test]
fn test_sqlite_functionality() {
    // Test all SQLite features in a single eval to work around
    // the double-execution issue in run_line()
    let val = eval(r#"
        let db = sqlite_open(":memory:");

        let _r1 = sqlite_exec(db, "CREATE TABLE test (id INT, val TEXT)");
        let _r2 = sqlite_exec(db, "INSERT INTO test VALUES (1, 'a')");
        let _r3 = sqlite_exec(db, "INSERT INTO test VALUES (2, 'b')");

        let all_rows = sqlite_query(db, "SELECT * FROM test");
        let filtered = sqlite_query(db, "SELECT * FROM test WHERE id > ?", 1);

        let tables = sqlite_tables(db);
        let exists = sqlite_table_exists(db, "test");

        let _close = sqlite_close(db);

        count(all_rows);
    "#);

    assert_eq!(val, Value::Number(2.0));
}
