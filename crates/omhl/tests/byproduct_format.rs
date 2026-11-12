//! Formato do blob de byproduct: `encode` e `decode` são as duas pontas do mesmo contrato.

use omhl::byproduct::{decode, encode};

#[test]
fn it_should_encode_the_header_followed_by_stdout_and_stderr() {
    let blob = encode(b"out\n", b"err\n");

    assert_eq!(blob, b"omhl-byproduct v1 stdout=4 stderr=4\nout\nerr\n");
}

#[test]
fn it_should_decode_what_it_encoded() {
    let cases: [(&[u8], &[u8]); 5] = [
        (b"out\n", b"err\n"),
        (b"", b""),
        (b"", b"only stderr\n"),
        (b"no trailing newline", b"nor here"),
        (b"line one\nline two\n", b""),
    ];

    for (stdout, stderr) in cases {
        let blob = encode(stdout, stderr);

        assert_eq!(decode(&blob), Some((stdout, stderr)));
    }
}

#[test]
fn it_should_refuse_a_blob_that_does_not_follow_the_format() {
    let blobs: [&[u8]; 5] = [
        b"",
        b"no header at all",
        b"omhl-byproduct v2 stdout=1 stderr=0\nx",
        b"omhl-byproduct v1 stdout=1 stderr=1\nx",
        b"omhl-byproduct v1 stdout=x stderr=0\n",
    ];

    let decoded: Vec<_> = blobs.iter().map(|blob| decode(blob)).collect();

    assert_eq!(decoded, vec![None; 5]);
}
