//! Integration tests.

extern crate assert_cmd;
extern crate escargot;
extern crate filetime;
extern crate predicates;
extern crate tempfile;

use std::fs::{create_dir_all, File};
use std::io::Write;
use std::process::Command;
use std::time::{Duration, SystemTime};

use assert_cmd::prelude::*;
use predicates::boolean::PredicateBooleanExt;
use predicates::prelude::predicate::str::{contains, is_empty, similar};
use tempfile::{Builder, TempDir};

struct TestEnv {
    pub cache_dir: TempDir,
    pub config_dir: TempDir,
    pub input_dir: TempDir,
    pub default_features: bool,
    pub features: Vec<String>,
}

impl TestEnv {
    fn new() -> Self {
        TestEnv {
            cache_dir: Builder::new().prefix(".tldr.test.cache").tempdir().unwrap(),
            config_dir: Builder::new().prefix(".tldr.test.conf").tempdir().unwrap(),
            input_dir: Builder::new().prefix(".tldr.test.input").tempdir().unwrap(),
            default_features: true,
            features: vec![],
        }
    }

    /// Add entry for that environment.
    fn add_entry(&self, name: &str, contents: &str) {
        let dir = self
            .cache_dir
            .path()
            .join("tldr-master")
            .join("pages")
            .join("common");
        create_dir_all(&dir).unwrap();

        let mut file = File::create(&dir.join(format!("{}.md", name))).unwrap();
        file.write_all(&contents.as_bytes()).unwrap();
    }

    /// Disable default features.
    #[allow(dead_code)] // Might be useful in the future
    fn no_default_features(mut self) -> Self {
        self.default_features = false;
        self
    }

    /// Add the specified feature.
    #[allow(dead_code)] // Might be useful in the future
    fn with_feature<S: Into<String>>(mut self, feature: S) -> Self {
        self.features.push(feature.into());
        self
    }

    /// Return a new `Command` with env vars set.
    fn command(&self) -> Command {
        let mut build = escargot::CargoBuild::new()
            .bin("tldr")
            .current_release()
            .current_target();
        if !self.default_features {
            build = build.arg("--no-default-features");
        }
        if !self.features.is_empty() {
            build = build.arg(&format!("--feature {}", self.features.join(",")));
        }
        let run = build.run().unwrap();
        let mut cmd = run.command();
        cmd.env(
            "TEALDEER_CACHE_DIR",
            self.cache_dir.path().to_str().unwrap(),
        );
        cmd.env(
            "TEALDEER_CONFIG_DIR",
            self.config_dir.path().to_str().unwrap(),
        );
        cmd
    }
}

#[test]
fn test_missing_cache() {
    TestEnv::new()
        .command()
        .args(&["sl"])
        .assert()
        .failure()
        .stderr(contains("Cache not found. Please run `tldr --update`."));
}

#[test]
fn test_update_cache() {
    let testenv = TestEnv::new();

    testenv
        .command()
        .args(&["sl"])
        .assert()
        .failure()
        .stderr(contains("Cache not found. Please run `tldr --update`."));

    testenv
        .command()
        .args(&["--update"])
        .assert()
        .success()
        .stdout(contains("Successfully updated cache."));

    testenv.command().args(&["sl"]).assert().success();
}

#[test]
fn test_quiet_cache() {
    let testenv = TestEnv::new();
    testenv
        .command()
        .args(&["--update", "--quiet"])
        .assert()
        .success()
        .stdout(is_empty());

    testenv
        .command()
        .args(&["--clear-cache", "--quiet"])
        .assert()
        .success()
        .stdout(is_empty());
}

#[test]
fn test_quiet_failures() {
    let testenv = TestEnv::new();

    testenv
        .command()
        .args(&["--update", "-q"])
        .assert()
        .success()
        .stdout(is_empty());

    testenv
        .command()
        .args(&["fakeprogram", "-q"])
        .assert()
        .failure()
        .stdout(is_empty());
}

#[test]
fn test_quiet_old_cache() {
    let testenv = TestEnv::new();

    testenv
        .command()
        .args(&["--update", "-q"])
        .assert()
        .success()
        .stdout(is_empty());

    filetime::set_file_mtime(
        testenv.cache_dir.path().join("tldr-master"),
        filetime::FileTime::from_unix_time(1, 0),
    )
    .unwrap();

    testenv
        .command()
        .args(&["tldr"])
        .assert()
        .success()
        .stderr(contains("The cache hasn't been updated for more than "));

    testenv
        .command()
        .args(&["tldr", "--quiet"])
        .assert()
        .success()
        .stderr(contains("The cache hasn't been updated for more than ").not());
}

#[test]
fn test_setup_seed_config() {
    let testenv = TestEnv::new();

    testenv
        .command()
        .args(&["--seed-config"])
        .assert()
        .success()
        .stdout(contains("Successfully created seed config file"));
}

#[test]
fn test_show_config_path() {
    let testenv = TestEnv::new();

    testenv
        .command()
        .args(&["--config-path"])
        .assert()
        .success()
        .stdout(contains(format!(
            "Config path is: {}",
            testenv
                .config_dir
                .path()
                .join("config.toml")
                .to_str()
                .unwrap(),
        )));
}

#[test]
fn test_markdown_rendering() {
    let testenv = TestEnv::new();

    testenv
        .command()
        .args(&["--update"])
        .assert()
        .success()
        .stdout(contains("Successfully updated cache."));

    let expected = include_str!("tar-markdown.expected");
    testenv
        .command()
        .args(&["-m", "tar"])
        .assert()
        .success()
        .stdout(similar(expected));
}

fn _test_correct_rendering(
    input_file: &str,
    filename: &str,
    expected: &'static str,
    color_option: &str,
) {
    let testenv = TestEnv::new();

    // Create input file
    let file_path = testenv.input_dir.path().join(filename);
    println!("Testfile path: {:?}", &file_path);
    let mut file = File::create(&file_path).unwrap();
    file.write_all(input_file.as_bytes()).unwrap();

    testenv
        .command()
        .args(&["--color", color_option, "-f", &file_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(similar(expected));
}

/// An end-to-end integration test for direct file rendering (v1 syntax).
#[test]
fn test_correct_rendering_v1() {
    _test_correct_rendering(
        include_str!("inkscape-v1.md"),
        "inkscape-v1.md",
        include_str!("inkscape-default.expected"),
        "always",
    );
}

/// An end-to-end integration test for direct file rendering (v2 syntax).
#[test]
fn test_correct_rendering_v2() {
    _test_correct_rendering(
        include_str!("inkscape-v2.md"),
        "inkscape-v2.md",
        include_str!("inkscape-default.expected"),
        "always",
    );
}

#[test]
/// An end-to-end integration test for direct file rendering with the `--color auto` option. This
/// will not use styling since output is not stdout.
fn test_rendering_color_auto() {
    _test_correct_rendering(
        include_str!("inkscape-v2.md"),
        "inkscape-v2.md",
        include_str!("inkscape-default-no-color.expected"),
        "auto",
    );
}

#[test]
/// An end-to-end integration test for direct file rendering with the `--color never` option.
fn test_rendering_color_never() {
    _test_correct_rendering(
        include_str!("inkscape-v2.md"),
        "inkscape-v2.md",
        include_str!("inkscape-default-no-color.expected"),
        "never",
    );
}

/// An end-to-end integration test for rendering with constom syntax config.
#[test]
fn test_correct_rendering_with_config() {
    let testenv = TestEnv::new();

    // Setup config file
    // TODO should be config::CONFIG_FILE_NAME
    let config_file_path = testenv.config_dir.path().join("config.toml");
    println!("Config path: {:?}", &config_file_path);

    let mut config_file = File::create(&config_file_path).unwrap();
    config_file
        .write(include_str!("config.toml").as_bytes())
        .unwrap();

    // Create input file
    let file_path = testenv.input_dir.path().join("inkscape-v2.md");
    println!("Testfile path: {:?}", &file_path);

    let mut file = File::create(&file_path).unwrap();
    file.write_all(include_str!("inkscape-v2.md").as_bytes())
        .unwrap();

    // Load expected output
    let expected = include_str!("inkscape-with-config.expected");

    testenv
        .command()
        .args(&["--color", "always", "-f", &file_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(similar(expected));
}

#[test]
fn test_spaces_find_command() {
    let testenv = TestEnv::new();

    testenv
        .command()
        .args(&["--update"])
        .assert()
        .success()
        .stdout(contains("Successfully updated cache."));

    testenv
        .command()
        .args(&["git", "checkout"])
        .assert()
        .success();
}

#[test]
fn test_pager_flag_enable() {
    let testenv = TestEnv::new();

    testenv
        .command()
        .args(&["--update"])
        .assert()
        .success()
        .stdout(contains("Successfully updated cache."));

    testenv
        .command()
        .args(&["--pager", "tar"])
        .assert()
        .success();
}

#[test]
fn test_list_flag_rendering() {
    let testenv = TestEnv::new();

    testenv
        .command()
        .args(&["--list"])
        .assert()
        .failure()
        .stderr(contains("Cache not found. Please run `tldr --update`."));

    testenv.add_entry("foo", "");

    testenv
        .command()
        .args(&["--list"])
        .assert()
        .success()
        .stdout("foo\n");

    testenv.add_entry("bar", "");
    testenv.add_entry("baz", "");
    testenv.add_entry("qux", "");

    testenv
        .command()
        .args(&["--list"])
        .assert()
        .success()
        .stdout("bar\nbaz\nfoo\nqux\n");
}

#[test]
fn test_autoupdate_cache() {
    let testenv = TestEnv::new();

    // The first time, if automatic updates are disabled, the cache should not be found
    testenv
        .command()
        .args(&["--list"])
        .assert()
        .failure()
        .stderr(contains("Cache not found. Please run `tldr --update`."));

    let config_file_path = testenv.config_dir.path().join("config.toml");
    let cache_file_path = testenv.cache_dir.path().join("tldr-master");

    // Activate automatic updates, set the auto-update interval to 24 hours
    let mut config_file = File::create(&config_file_path).unwrap();
    config_file
        .write("[updates]\nauto_update = true\nauto_update_interval_hours = 24".as_bytes())
        .unwrap();
    config_file.flush().unwrap();

    // Helper function that runs `tldr --list` and asserts that the cache is automatically updated
    // or not, depending on the value of `expected`.
    let check_cache_updated = |expected| {
        let assert = testenv.command().args(&["--list"]).assert().success();
        let pred = contains("Successfully updated cache");
        if expected {
            assert.stdout(pred)
        } else {
            assert.stdout(pred.not())
        };
    };

    // The cache is updated the first time we run `tldr --list`
    check_cache_updated(true);

    // The cache is not updated with a subsequent call
    check_cache_updated(false);

    // We update the modification and access times such that they are about 23 hours from now.
    // auto-update interval is 24 hours, the cache should not be updated
    let new_mtime = SystemTime::now() - Duration::from_secs(82_800);
    filetime::set_file_mtime(&cache_file_path, new_mtime.into()).unwrap();
    check_cache_updated(false);

    // We update the modification and access times such that they are about 25 hours from now.
    // auto-update interval is 24 hours, the cache should be updated
    let new_mtime = SystemTime::now() - Duration::from_secs(90_000);
    filetime::set_file_mtime(&cache_file_path, new_mtime.into()).unwrap();
    check_cache_updated(true);

    // The cache is not updated with a subsequent call
    check_cache_updated(false);
}

#[test]
#[cfg(target_os = "windows")]
fn test_pager_warning() {
    let testenv = TestEnv::new();
    testenv
        .command()
        .args(&["--update"])
        .assert()
        .success()
        .stdout(contains("Successfully updated cache."));

    // Regular call should not show a "pager flag not available on windows" warning
    testenv
        .command()
        .args(&["tar"])
        .assert()
        .success()
        .stderr(contains("pager flag not available on Windows").not());

    // But it should be shown if the pager flag is true
    testenv
        .command()
        .args(&["tar", "-p"])
        .assert()
        .success()
        .stderr(contains("pager flag not available on Windows"));
}
