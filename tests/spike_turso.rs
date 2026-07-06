fn test_tdf() -> Option<String> {
    std::env::var("TIMSRUST_CAL_TEST_TDF").ok()
}

#[test]
fn turso_reads_mzcalibration_and_frames_t1() {
    let Some(tdf) = test_tdf() else {
        eprintln!("skip: set TIMSRUST_CAL_TEST_TDF to a .tdf path");
        return;
    };
    pollster::block_on(async {
        let db = turso::Builder::new_local(&tdf).build().await.unwrap();
        let conn = db.connect().unwrap();

        // MzCalibration: at least one row, model_type present.
        let mut rows = conn
            .query("SELECT Id, ModelType, C0, C1 FROM MzCalibration", ())
            .await
            .unwrap();
        let row = rows.next().await.unwrap().expect("no MzCalibration rows");
        let model_type: i64 = row.get_value(1).unwrap().as_integer().copied().unwrap();
        assert_eq!(model_type, 1);

        // Frames: T1 column readable.
        let mut frows = conn
            .query("SELECT Id, T1, MzCalibration FROM Frames", ())
            .await
            .unwrap();
        let mut n = 0;
        while let Some(_f) = frows.next().await.unwrap() {
            n += 1;
        }
        assert!(n > 0, "no frames read");
    });
}
