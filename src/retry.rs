use rand::Rng;

pub fn backoff(retries: i32) -> i64 {
    let base: i64 = if retries > 19 {
        return 2851203;
    } else {
        (1.52f64.powi(retries) * 1000f64).round() as i64
    };

    base + rand::thread_rng().gen_range(0..1000)
}

#[test]
fn test_backoff() {
    let backoff = backoff(19);

    assert!(backoff >= 2851203);
    assert!(backoff < 2852203);
}

#[test]
fn test_backoff_max() {
    let backoff = backoff(20);

    assert!(backoff >= 2851203);
    assert!(backoff < 2852203);
}
