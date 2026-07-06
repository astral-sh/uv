use std::hint::black_box;
use std::str::FromStr;

use criterion::{Criterion, criterion_group, criterion_main, measurement::WallTime};
use uv_pep440::VersionSpecifiers;

fn parse_version_specifiers(c: &mut Criterion<WallTime>) {
    for specifiers in [">=3.8", ">=3.8,<4", ">=2.5, !=3.0.*, !=3.1.*, !=3.2.*, <4"] {
        let name = format!("parse_version_specifiers {specifiers}");
        c.bench_function(&name, |benchmark| {
            benchmark.iter(|| {
                VersionSpecifiers::from_str(black_box(specifiers))
                    .expect("benchmark input should be valid")
            });
        });
    }
}

criterion_group!(simulation, parse_version_specifiers);
criterion_main!(simulation);
