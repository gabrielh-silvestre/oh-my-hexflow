use crate::provider::{RepoRef, repo_from_origin_url};

fn widgets() -> RepoRef {
    RepoRef {
        host: "github.com".to_owned(),
        path: vec!["acme".to_owned(), "widgets".to_owned()],
    }
}

fn path_of(url: &str, host: &str) -> Option<Vec<String>> {
    repo_from_origin_url(url, host).map(|repo| repo.path)
}

#[test]
fn it_should_spell_the_slug_as_host_owner_and_repo() {
    let repo = widgets();

    let slug = repo.slug();

    assert_eq!(slug, "github.com/acme/widgets");
}

#[test]
fn it_should_read_owner_and_repo_from_an_https_origin() {
    let path = path_of("https://github.com/acme/widgets.git", "github.com");

    assert_eq!(path, Some(widgets().path));
}

#[test]
fn it_should_read_owner_and_repo_from_an_ssh_origin() {
    let path = path_of("git@github.com:acme/widgets.git", "github.com");

    assert_eq!(path, Some(widgets().path));
}

#[test]
fn it_should_read_nothing_from_an_origin_on_another_host() {
    let path = path_of("https://gitlab.com/acme/widgets.git", "github.com");

    assert_eq!(path, None);
}

#[test]
fn it_should_read_nothing_from_an_origin_with_more_than_two_segments() {
    let path = path_of("https://github.com/acme/group/widgets.git", "github.com");

    assert_eq!(path, None);
}
