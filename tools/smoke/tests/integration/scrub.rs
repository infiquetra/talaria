//! Scrubbing: every private path and name becomes a placeholder, in one pass, never rescanned.
//! Paths and names are built at run time from temporary directories; none is a literal.

use std::fs;
use std::path::{Path, PathBuf};

use proptest::prelude::*;
use proptest::test_runner::{Config, TestCaseError, TestRunner};
use tempfile::TempDir;

use smoke::{Scrubber, public_safety_violations};

/// A random name of letters and digits, at least ten characters long, with a readable prefix.
fn generated_name(prefix: &str) -> String {
    let dir = tempfile::Builder::new()
        .prefix(prefix)
        .rand_bytes(8)
        .tempdir()
        .expect("a temporary directory gives a random name");
    dir.path()
        .file_name()
        .expect("a directory name")
        .to_string_lossy()
        .into_owned()
}

fn text_of(path: &Path) -> String {
    path.to_str().expect("a UTF-8 path").to_owned()
}

fn real_text_of(path: &Path) -> String {
    text_of(&path.canonicalize().expect("the path exists"))
}

#[test]
fn prompt_shortened_scratch_paths_are_scrubbed_in_one_pass() {
    let plant = Plant::new();
    let name = plant.scratch.file_name().unwrap().to_str().unwrap();
    let shortened = ["private", "var", "..", "bucket", "..", "T", name].join("/");
    let scratch = text_of(&plant.scratch);
    let home = text_of(plant.home.path());
    let scrubber = Scrubber::new(&home, &plant.login, &plant.scratch, None, plant.tmp.path());
    let once = scrubber.scrub(&format!(
        "prompt {shortened} % {shortened}/child {scratch}/file {home}/documents"
    ));
    assert_eq!(
        once,
        "prompt <scratch> % <scratch>/child <scratch>/file <home>/documents"
    );
    assert_eq!(scrubber.scrub(&once), once);
}

#[test]
fn longer_exact_paths_below_scratch_keep_their_placeholder() {
    let plant = Plant::new();
    let home = text_of(plant.home.path());
    let scratch = text_of(&plant.scratch);
    let hermes = format!("{scratch}/.hermes");
    let scrubber = Scrubber::new(
        &home,
        &plant.login,
        &plant.scratch,
        Some(&hermes),
        plant.tmp.path(),
    );
    assert_eq!(
        scrubber.scrub(&format!("{hermes}/config {scratch}/file {home}/documents")),
        "<hermes-home>/config <scratch>/file <home>/documents"
    );
}

#[test]
fn trailing_path_separators_are_preserved_after_placeholders() {
    let plant = Plant::new();
    let tmp = format!("{}/", text_of(plant.tmp.path()));
    let scratch = format!("{}/", text_of(&plant.scratch));
    let scrubber = Scrubber::new(
        &text_of(plant.home.path()),
        &plant.login,
        Path::new(&scratch),
        None,
        Path::new(&tmp),
    );
    assert_eq!(
        scrubber.scrub(&format!("{tmp}report.txt {scratch}file.txt")),
        "<tmp>/report.txt <scratch>/file.txt"
    );
}

/// Four unrelated directories and a generated login name, so no needle holds another.
struct Plant {
    tmp: TempDir,
    home: TempDir,
    hermes: TempDir,
    _scratch_root: TempDir,
    scratch: PathBuf,
    login: String,
}

impl Plant {
    fn new() -> Self {
        let scratch_root = tempfile::tempdir().expect("scratch root");
        let scratch = scratch_root.path().join(generated_name("talaria-smoke-"));
        fs::create_dir(&scratch).expect("scratch directory");
        Self {
            tmp: tempfile::tempdir().expect("tmp"),
            home: tempfile::tempdir().expect("home"),
            hermes: tempfile::tempdir().expect("hermes home"),
            _scratch_root: scratch_root,
            scratch,
            login: generated_name("login"),
        }
    }

    fn scrubber(&self) -> Scrubber {
        Scrubber::new(
            &text_of(self.home.path()),
            &self.login,
            &self.scratch,
            Some(&text_of(self.hermes.path())),
            self.tmp.path(),
        )
    }

    /// Every needle the scrubber is expected to remove (scratch and tmp also resolved).
    fn needles(&self) -> Vec<String> {
        vec![
            text_of(&self.scratch),
            real_text_of(&self.scratch),
            text_of(self.tmp.path()),
            real_text_of(self.tmp.path()),
            text_of(self.home.path()),
            text_of(self.hermes.path()),
            self.login.clone(),
        ]
    }
}

#[test]
fn replaces_every_needle_kind() {
    let plant = Plant::new();
    let scrubber = plant.scrubber();
    let scratch = text_of(&plant.scratch);
    let tmp = text_of(plant.tmp.path());
    let home = text_of(plant.home.path());
    let hermes = text_of(plant.hermes.path());
    let login = &plant.login;

    assert_eq!(
        scrubber.scrub(&format!("cwd {scratch}/CACHEDIR.TAG ok")),
        "cwd <scratch>/CACHEDIR.TAG ok"
    );
    assert_eq!(
        scrubber.scrub(&format!("report in {tmp}/smoke-report.txt")),
        "report in <tmp>/smoke-report.txt"
    );
    assert_eq!(
        scrubber.scrub(&format!("config at {home}/settings")),
        "config at <home>/settings"
    );
    assert_eq!(
        scrubber.scrub(&format!("{login}@host % ls")),
        "<user>@host % ls"
    );
    assert_eq!(
        scrubber.scrub(&format!("state in {hermes}/profiles")),
        "state in <hermes-home>/profiles"
    );

    // the resolved real path of the scratch and temporary directories gets the same placeholder
    let real_scratch = real_text_of(&plant.scratch);
    let real_tmp = real_text_of(plant.tmp.path());
    assert_eq!(scrubber.scrub(&format!("{real_scratch}/x")), "<scratch>/x");
    assert_eq!(scrubber.scrub(&format!("{real_tmp}/x")), "<tmp>/x");

    // several kinds and several occurrences in one text
    let text = format!("{scratch}\n{home} {login} {login}\n{tmp}|{hermes}\n{real_scratch}");
    assert_eq!(
        scrubber.scrub(&text),
        "<scratch>\n<home> <user> <user>\n<tmp>|<hermes-home>\n<scratch>"
    );

    // text that holds no needle is returned as it is
    let plain = "SMOKE_RESULT: PASSED (elapsed 41 s)\nPASS startup 3.2 s";
    assert_eq!(scrubber.scrub(plain), plain);
    assert_eq!(scrubber.scrub(""), "");
}

#[test]
fn longest_needle_wins() {
    // the scratch project and the Hermes home both sit inside the home directory
    let home = tempfile::tempdir().expect("home");
    let tmp = tempfile::tempdir().expect("tmp");
    let home_text = text_of(home.path());
    let scratch = home.path().join(generated_name("talaria-smoke-"));
    fs::create_dir(&scratch).expect("scratch directory");
    let hermes = format!("{home_text}/.hermes");
    let login = generated_name("login");
    let scrubber = Scrubber::new(&home_text, &login, &scratch, Some(&hermes), tmp.path());
    let scratch_text = text_of(&scratch);

    assert_eq!(
        scrubber.scrub(&format!(
            "{scratch_text}/a {home_text}/b {hermes}/config.yaml {home_text}"
        )),
        "<scratch>/a <home>/b <hermes-home>/config.yaml <home>"
    );
    // the home directory alone still becomes the home placeholder
    assert_eq!(
        scrubber.scrub(&format!("{home_text}/documents")),
        "<home>/documents"
    );

    // the same holds for the resolved path of the scratch project
    let real_scratch = real_text_of(&scratch);
    assert_eq!(
        scrubber.scrub(&format!("{real_scratch}/a {home_text}/b")),
        "<scratch>/a <home>/b"
    );

    // a login name inside a longer path is part of the path needle, not a second hit
    let named_home = format!("{}/{login}", text_of(tmp.path()));
    let scrubber = Scrubber::new(&named_home, &login, &scratch, None, tmp.path());
    assert_eq!(
        scrubber.scrub(&format!("{named_home}/x {login}")),
        "<home>/x <user>"
    );
}

#[test]
fn short_login_name_is_skipped() {
    let tmp = tempfile::tempdir().expect("tmp");
    let home = tempfile::tempdir().expect("home");
    let scratch = tmp.path().join(generated_name("talaria-smoke-"));
    fs::create_dir(&scratch).expect("scratch directory");
    let home_text = text_of(home.path());
    let long = generated_name("login");

    // three characters: skipped, ordinary words stay whole
    let short = &long[..3];
    let scrubber = Scrubber::new(&home_text, short, &scratch, None, tmp.path());
    let text = format!("{short} {short}{short} the word {short}ly and {short}");
    assert_eq!(scrubber.scrub(&text), text);
    // the other needles still work
    assert_eq!(
        scrubber.scrub(&format!("{short} {home_text}/x")),
        format!("{short} <home>/x")
    );

    // two characters and the empty name: skipped
    for name in [&long[..2], &long[..1], ""] {
        let scrubber = Scrubber::new(&home_text, name, &scratch, None, tmp.path());
        let text = format!("ordinary text {name} with name {name}{name}");
        assert_eq!(scrubber.scrub(&text), text, "login {name:?}");
    }

    // four characters is long enough
    let four = &long[..4];
    let scrubber = Scrubber::new(&home_text, four, &scratch, None, tmp.path());
    assert_eq!(scrubber.scrub(&format!("x {four} y")), "x <user> y");

    // a home directory shorter than four characters is skipped as well
    for short_home in ["", "/", "/a"] {
        let scrubber = Scrubber::new(short_home, &long, &scratch, None, tmp.path());
        let text = "a/b/c and /a/b and plain words";
        assert_eq!(scrubber.scrub(text), text, "home {short_home:?}");
    }
}

#[test]
fn needle_inside_a_placeholder_is_skipped() {
    let tmp = tempfile::tempdir().expect("tmp");
    let home = tempfile::tempdir().expect("home");
    let scratch = tmp.path().join(generated_name("talaria-smoke-"));
    fs::create_dir(&scratch).expect("scratch directory");
    let home_text = text_of(home.path());

    // ordinary words that sit inside a placeholder word, and text holding an angle bracket
    for login in [
        "user", "home", "scratch", "hermes", "ermes", "mes-h", "<abc>", "a<bcd", "wxyz>",
    ] {
        let scrubber = Scrubber::new(&home_text, login, &scratch, None, tmp.path());

        // the word itself is left alone
        let word = format!("{login} says hello");
        assert_eq!(scrubber.scrub(&word), word, "login {login:?}");

        // other needles are still replaced, and the output is stable
        let text = format!("{login} lives in {home_text}/work");
        let once = scrubber.scrub(&text);
        assert_eq!(
            once,
            format!("{login} lives in <home>/work"),
            "login {login:?}"
        );
        assert_eq!(scrubber.scrub(&once), once, "login {login:?}");
    }

    // text that already holds placeholders is not changed by a second scrub
    let scrubber = Scrubber::new(
        &home_text,
        "user",
        &scratch,
        Some("hermes-home"),
        tmp.path(),
    );
    let placeholders = "see <user> and <scratch> and <tmp> and <home> and <hermes-home> here";
    assert_eq!(scrubber.scrub(placeholders), placeholders);
    let mixed = format!("{home_text}/x for user and home and <user>");
    let once = scrubber.scrub(&mixed);
    assert_eq!(once, "<home>/x for user and home and <user>");
    assert_eq!(scrubber.scrub(&once), once);
}

#[test]
fn violations_report_planted_paths_names() {
    let tmp = tempfile::tempdir().expect("tmp");
    let planted_path = text_of(tmp.path());
    let login = generated_name("login");
    let other = generated_name("other");
    let needles = vec![login.clone(), planted_path.clone()];

    // clean text finds nothing, whatever the needles are
    let clean = "tab: w1:t5\nPASS startup 3.2 s\nSMOKE_RESULT: PASSED (elapsed 41 s)";
    assert!(public_safety_violations(clean, &needles).is_empty());
    assert!(public_safety_violations(clean, &[]).is_empty());
    assert!(public_safety_violations("", &needles).is_empty());

    // a planted absolute path is reported without any needle
    let path_text = format!("cwd: {planted_path}\nnext line");
    assert!(!public_safety_violations(&path_text, &[]).is_empty());
    let generated_path = format!(
        "/{}/{}/file.txt",
        generated_name("dir"),
        generated_name("sub")
    );
    assert!(!public_safety_violations(&format!("opened {generated_path}"), &[]).is_empty());
    assert!(!public_safety_violations(&generated_path, &[]).is_empty());

    // a planted name is reported when it is a needle
    let name_text = format!("prompt {login}@host % ls");
    assert!(!public_safety_violations(&name_text, std::slice::from_ref(&login)).is_empty());
    assert!(!public_safety_violations(&name_text, &needles).is_empty());
    assert!(!public_safety_violations(login.as_str(), &needles).is_empty());

    // a needle that is not in the text, and a name that is not a needle, are not reported
    assert!(public_safety_violations(&name_text, std::slice::from_ref(&other)).is_empty());
    assert!(public_safety_violations("nothing private here", &[login, other]).is_empty());
}

fn proptest_config() -> Config {
    Config {
        cases: 64,
        max_shrink_iters: 0,
        failure_persistence: None,
        ..Config::default()
    }
}

#[test]
fn property_scrub_idempotent_no_needle_survives() {
    let plant = Plant::new();
    let scrubber = plant.scrubber();
    let needles = plant.needles();
    let mut pieces = needles.clone();
    pieces.extend(
        [
            "<scratch>",
            "<home>",
            "<user>",
            "<tmp>",
            "<hermes-home>",
            "user",
            "home",
            "tmp",
            "/",
            "x",
            " ",
            "\n",
            "abc",
            "word",
            ".",
            "-",
            "<",
            ">",
        ]
        .map(str::to_owned),
    );
    let strategy = prop::collection::vec(prop::sample::select(pieces), 0..30);

    let mut runner = TestRunner::new(proptest_config());
    runner
        .run(&strategy, |chosen| {
            let text = chosen.concat();
            let once = scrubber.scrub(&text);
            for needle in &needles {
                prop_assert!(
                    !once.contains(needle.as_str()),
                    "needle {needle:?} survived in {once:?}"
                );
            }
            prop_assert_eq!(scrubber.scrub(&once), once.clone());
            Ok(())
        })
        .expect("scrubbing is idempotent and leaves no needle");
}

#[test]
fn property_unrelated_text_is_unchanged() {
    let plant = Plant::new();
    let scrubber = plant.scrubber();
    let needles = plant.needles();
    let strategy = "[a-zA-Z0-9 .,:;!?<>()\\n-]{0,120}";

    let mut runner = TestRunner::new(proptest_config());
    runner
        .run(&strategy, |text| {
            if needles.iter().any(|needle| text.contains(needle.as_str())) {
                return Err(TestCaseError::reject("the text holds a needle"));
            }
            prop_assert_eq!(scrubber.scrub(&text), text);
            Ok(())
        })
        .expect("text without a needle is unchanged");
}
