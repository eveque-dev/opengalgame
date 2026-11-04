//! Record a reproducible acceptance route using the public engine API.
use opengalgame::{
    debugger::{Command, Route},
    model::NodeKind,
    project_io::*,
    runtime::Runtime,
};
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let path = args
        .first()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples/harbor-signals/project.json")
        });
    let output = args
        .get(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("reports/promise.route.json"));
    let p = read_project(&path)?;
    let mut runtime = Runtime::new(&p);
    let mut route = Route::new(&p, &runtime);
    for _ in 0..100 {
        let n = p.node(&runtime.current.node).unwrap();
        if n.kind == NodeKind::Ending {
            break;
        }
        let command = Command::Advance {
            choice: (n.kind == NodeKind::Choice).then_some(0),
        };
        command.apply(&p, &mut runtime)?;
        route.record(command, &runtime)?;
    }
    assert_eq!(runtime.current.node, "ending_promise");
    assert_eq!(route.replay(&p)?, runtime);
    write_json(&output, &route)?;
    println!(
        "Recorded {} commands to {}",
        route.commands.len(),
        output.display()
    );
    Ok(())
}
