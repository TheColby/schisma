use schisma_gpu::{discover, Accelerator, BackendKind};

fn main() {
    for status in discover() {
        println!(
            "{:>5}: {:<11} {:<28} {}",
            status.kind.label(),
            if status.available {
                "available"
            } else {
                "unavailable"
            },
            status.device_name,
            status.detail
        );
    }

    let requested = std::env::args()
        .nth(1)
        .as_deref()
        .map(parse_backend)
        .unwrap_or(BackendKind::Auto);
    let mut accelerator = Accelerator::new(requested);
    println!(
        "selected: {} via {}",
        accelerator.active().label(),
        accelerator.device_name()
    );
    if let Some(reason) = accelerator.fallback_reason() {
        println!("fallback: {reason}");
    }
    accelerator.self_test().expect("GPU self-test failed");
    println!("self-test: passed");
}

fn parse_backend(value: &str) -> BackendKind {
    match value.to_ascii_lowercase().as_str() {
        "cpu" => BackendKind::Cpu,
        "metal" => BackendKind::Metal,
        "cuda" => BackendKind::Cuda,
        _ => BackendKind::Auto,
    }
}
