//! Generate regression cases and a route after creating the narrated demo assets.
use opengalgame::{debugger::Route, project_io::*, runtime::Runtime, testing};
fn main() -> anyhow::Result<()> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/tidal-stage/project.json");
    let mut p = read_project(&path)?;
    let report = testing::run(&p, 8192);
    anyhow::ensure!(
        report.passed() && report.endings.len() == 4,
        "Demo routes are incomplete"
    );
    p.production.cases = report.routes;
    save_project(&path, &p)?;
    let case = &p.production.cases[0];
    let mut runtime = Runtime::new(&p);
    let mut route = Route::new(&p, &runtime);
    for command in &case.commands {
        command.apply(&p, &mut runtime)?;
        route.record(command.clone(), &runtime)?;
    }
    route.replay(&p)?;
    write_json(&path.with_file_name("route-demo.json"), &route)?;
    write_json(
        &path.with_file_name("test-report.json"),
        &testing::run(&p, 8192),
    )?;
    println!("Recorded {} regression cases", p.production.cases.len());
    Ok(())
}
