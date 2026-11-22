use crate::mechanical::sweep::{Candidate, RunMarker, root_hash, select_strays, start_from_stat};
use std::path::Path;

/// Namespace de pid do varredor nos testes.
const NS: u64 = 4026531836;
/// Raiz do projeto do varredor nos testes.
const ROOT: &str = "a1b2c3d4e5f60718";
const OTHER_ROOT: &str = "0f0e0d0c0b0a0908";

fn cand(pid: u32, pgrp: u32, marker: &str) -> Candidate {
    Candidate {
        pid,
        start: 5000,
        pgrp,
        marker: marker.to_string(),
    }
}

fn pids(selected: &[Candidate]) -> Vec<u32> {
    selected.iter().map(|c| c.pid).collect::<Vec<u32>>()
}

fn marker_fields(marker: Option<RunMarker>) -> Option<(u32, u64, u64)> {
    marker.map(|m| (m.pid, m.start, m.pidns))
}

#[test]
fn it_should_encode_and_parse_a_run_marker() {
    let marker = RunMarker {
        pid: 4242,
        start: 987654,
        pidns: 4026531836,
        root: ROOT.to_owned(),
    };
    let extreme = RunMarker {
        pid: 4294967295,
        start: 18446744073709551615,
        pidns: 18446744073709551615,
        root: ROOT.to_owned(),
    };

    let encoded = marker.encode();
    let parsed = RunMarker::parse("v1:4242:987654:4026531836:a1b2c3d4e5f60718");
    let extreme_encoded = extreme.encode();
    let extreme_parsed = RunMarker::parse(&extreme_encoded);

    assert_eq!(encoded, "v1:4242:987654:4026531836:a1b2c3d4e5f60718");
    assert_eq!(marker_fields(parsed), Some((4242, 987654, 4026531836)));
    assert_eq!(
        extreme_encoded,
        "v1:4294967295:18446744073709551615:18446744073709551615:a1b2c3d4e5f60718"
    );
    assert_eq!(
        marker_fields(extreme_parsed),
        Some((4294967295, 18446744073709551615, 18446744073709551615))
    );
}

#[test]
fn it_should_reject_a_malformed_run_marker() {
    let texts = [
        "",
        "v1",
        "v1:4242:987654",
        "v2:4242:987654:4026531836",
        "V1:4242:987654:4026531836",
        "v1:4242:987654:4026531836",
        "v1:4242:987654:4026531836:9",
        "v1:4242:987654:4026531836:A1B2C3D4E5F60718",
        "v1:4242:987654:4026531836:a1b2c3d4e5f6071",
        "v1:4242:987654:4026531836:a1b2c3d4e5f60718:9",
        "v1::987654:4026531836",
        "v1:-1:987654:4026531836",
        "v1:+4242:987654:4026531836",
        "v1:4242:98a654:4026531836",
        " v1:4242:987654:4026531836:a1b2c3d4e5f60718",
        "v1:4242:987654:4026531836\n",
        "v1:4294967296:987654:4026531836:a1b2c3d4e5f60718",
        "v1:4242:18446744073709551616:4026531836:a1b2c3d4e5f60718",
        "v1:４２４２:987654:4026531836",
    ];
    let candidates: Vec<Candidate> = (300u32..)
        .zip(texts)
        .map(|(pid, text)| cand(pid, pid, text))
        .collect();

    let accepted: Vec<&str> = texts
        .into_iter()
        .filter(|text| RunMarker::parse(text).is_some())
        .collect();
    let selected = select_strays(100, None, NS, None, &candidates, &|_| None, &|_| false);

    assert_eq!(accepted, Vec::<&str>::new());
    assert_eq!(pids(&selected), Vec::<u32>::new());
}

#[test]
fn it_should_read_the_marker_start_time_after_the_last_parenthesis_of_a_stat_line() {
    let line =
        "4242 (a) b (c) S 1 4240 4240 0 -1 4194560 100 0 0 0 0 0 0 0 20 0 1 0 987654 1000 10\n";

    let start = start_from_stat(line);

    assert_eq!(start, Some(987654));
}

#[test]
fn it_should_select_for_the_sweep_a_process_whose_owner_is_gone() {
    let candidates = [cand(300, 300, "v1:200:1000:4026531836:a1b2c3d4e5f60718")];

    let selected = select_strays(100, None, NS, None, &candidates, &|_| None, &|_| false);

    assert_eq!(pids(&selected), vec![300]);
}

#[test]
fn it_should_spare_from_the_sweep_a_process_whose_owner_is_alive() {
    let candidates = [cand(300, 300, "v1:200:1000:4026531836:a1b2c3d4e5f60718")];
    let owner_start = |p: u32| if p == 200 { Some(1000) } else { None };

    let selected = select_strays(100, None, NS, None, &candidates, &owner_start, &|_| false);

    assert_eq!(pids(&selected), Vec::<u32>::new());
}

#[test]
fn it_should_sweep_a_process_whose_owner_pid_was_reused_by_a_later_process() {
    let candidates = [cand(300, 300, "v1:200:1000:4026531836:a1b2c3d4e5f60718")];
    let owner_start = |p: u32| if p == 200 { Some(1001) } else { None };

    let selected = select_strays(100, None, NS, None, &candidates, &owner_start, &|_| false);

    assert_eq!(pids(&selected), vec![300]);
}

#[test]
fn it_should_spare_its_own_process_and_its_own_lineage_from_the_sweep() {
    let candidates = [
        cand(300, 300, "v1:202:1000:4026531836:a1b2c3d4e5f60718"),
        cand(301, 301, "v1:200:1000:4026531836:a1b2c3d4e5f60718"),
        cand(302, 302, "v1:201:1000:4026531836:a1b2c3d4e5f60718"),
    ];

    let selected = select_strays(
        300,
        Some("v1:200:1000:4026531836:a1b2c3d4e5f60718"),
        NS,
        None,
        &candidates,
        &|_| None,
        &|_| false,
    );

    assert_eq!(pids(&selected), vec![302]);
}

#[test]
fn it_should_spare_from_the_sweep_a_process_whose_group_leader_is_alive_without_a_marker() {
    let candidates = [
        cand(300, 299, "v1:200:1000:4026531836:a1b2c3d4e5f60718"),
        cand(310, 310, "v1:200:1000:4026531836:a1b2c3d4e5f60718"),
    ];

    let selected = select_strays(100, None, NS, None, &candidates, &|_| None, &|_| true);

    assert_eq!(pids(&selected), vec![310]);
}

#[test]
fn it_should_select_for_the_sweep_a_process_whose_group_leader_is_gone() {
    let candidates = [cand(300, 299, "v1:200:1000:4026531836:a1b2c3d4e5f60718")];

    let selected = select_strays(100, None, NS, None, &candidates, &|_| None, &|_| false);

    assert_eq!(pids(&selected), vec![300]);
}

#[test]
fn it_should_spare_from_the_sweep_a_marker_from_another_pid_namespace() {
    let candidates = [
        cand(300, 300, "v1:200:1000:4026532219:a1b2c3d4e5f60718"),
        cand(301, 301, "v1:201:1000:4026531836:a1b2c3d4e5f60718"),
    ];

    let selected = select_strays(100, None, NS, None, &candidates, &|_| None, &|_| false);

    assert_eq!(pids(&selected), vec![301]);
}

#[test]
fn it_should_spare_from_the_sweep_a_dead_owner_marker_from_another_project_root() {
    let candidates = [
        cand(300, 300, &format!("v1:200:1000:4026531836:{OTHER_ROOT}")),
        cand(301, 301, &format!("v1:201:1000:4026531836:{ROOT}")),
    ];

    let selected = select_strays(100, None, NS, Some(ROOT), &candidates, &|_| None, &|_| {
        false
    });

    assert_eq!(pids(&selected), vec![301]);
}

#[test]
fn it_should_sweep_dead_owner_markers_from_every_project_root_when_no_root_is_given() {
    let candidates = [
        cand(300, 300, &format!("v1:200:1000:4026531836:{OTHER_ROOT}")),
        cand(301, 301, &format!("v1:201:1000:4026531836:{ROOT}")),
    ];

    let selected = select_strays(100, None, NS, None, &candidates, &|_| None, &|_| false);

    assert_eq!(pids(&selected), vec![300, 301]);
}

#[test]
fn it_should_derive_the_project_root_hash_from_the_path() {
    let first = root_hash(Path::new("/work/repo-a"));
    let again = root_hash(Path::new("/work/repo-a"));
    let other = root_hash(Path::new("/work/repo-b"));

    assert_eq!(first, again);
    assert_ne!(first, other);
    assert_eq!(first.len(), 16);
    assert!(RunMarker::parse(&format!("v1:1:1:1:{first}")).is_some());
}
