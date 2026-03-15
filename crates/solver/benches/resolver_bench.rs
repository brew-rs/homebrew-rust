use brew_solver::resolver::{PackageEntry, SATResolver};
use criterion::{criterion_group, criterion_main, Criterion};
use semver::Version;

/// Build a linear chain: pkg-0 → pkg-1 → … → pkg-(n-1)
fn linear_chain(n: usize) -> SATResolver {
    let mut resolver = SATResolver::new();
    for i in 0..n {
        let deps = if i + 1 < n {
            vec![brew_formula::Dependency::new(format!("pkg-{}", i + 1))]
        } else {
            vec![]
        };
        resolver.add_package(PackageEntry {
            name: format!("pkg-{}", i),
            version: Version::new(1, 0, 0),
            dependencies: deps,
        });
    }
    resolver.require("pkg-0");
    resolver
}

/// Build a diamond graph with `width` parallel paths of depth `depth`.
/// Root → layer[0][0..width] → … → leaf
fn diamond_graph(width: usize, depth: usize) -> SATResolver {
    let mut resolver = SATResolver::new();

    // Leaf package
    resolver.add_package(PackageEntry {
        name: "leaf".to_string(),
        version: Version::new(1, 0, 0),
        dependencies: vec![],
    });

    // Middle layers
    for layer in 0..depth {
        for w in 0..width {
            let name = format!("layer-{}-{}", layer, w);
            let dep_name = if layer + 1 < depth {
                format!("layer-{}-{}", layer + 1, w)
            } else {
                "leaf".to_string()
            };
            resolver.add_package(PackageEntry {
                name,
                version: Version::new(1, 0, 0),
                dependencies: vec![brew_formula::Dependency::new(dep_name)],
            });
        }
    }

    // Root depends on all top-layer nodes
    let root_deps: Vec<brew_formula::Dependency> = (0..width)
        .map(|w| brew_formula::Dependency::new(format!("layer-0-{}", w)))
        .collect();
    resolver.add_package(PackageEntry {
        name: "root".to_string(),
        version: Version::new(1, 0, 0),
        dependencies: root_deps,
    });
    resolver.require("root");
    resolver
}

fn bench_10_package_linear(c: &mut Criterion) {
    c.bench_function("resolver_linear_10", |b| {
        b.iter(|| {
            let resolver = linear_chain(10);
            resolver.resolve().unwrap();
        });
    });
}

fn bench_50_package_diamond(c: &mut Criterion) {
    // 5 parallel paths × 10 layers deep = ~51 packages
    c.bench_function("resolver_diamond_50", |b| {
        b.iter(|| {
            let resolver = diamond_graph(5, 10);
            resolver.resolve().unwrap();
        });
    });
}

fn bench_100_package_stress(c: &mut Criterion) {
    // 10 parallel paths × 10 layers deep = ~101 packages
    c.bench_function("resolver_stress_100", |b| {
        b.iter(|| {
            let resolver = diamond_graph(10, 10);
            resolver.resolve().unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_10_package_linear,
    bench_50_package_diamond,
    bench_100_package_stress
);
criterion_main!(benches);
