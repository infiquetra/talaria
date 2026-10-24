//! Build the services for one launch and run until the process should exit.

use talaria_app::app::ComposerDeadline;
use talaria_app::launch::Launch;

pub fn run(launch: Launch) -> std::process::ExitCode {
    let _ = launch;
    todo!("binary")
}

pub fn composer_deadline(
    composer: &mut talaria_tui::composer::Composer,
) -> &mut dyn ComposerDeadline {
    let _ = composer;
    todo!("binary")
}

pub fn status_service_slot(service: &mut talaria_tui::status::StatusCommandService) {
    let _ = service;
    todo!("binary")
}
