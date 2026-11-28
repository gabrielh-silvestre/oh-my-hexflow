---
description: Create the omhl commit mark so the next commit on a dry-run-with-mark exception branch can run the before-commit checks
---

Create the commit mark of this checkout. Run exactly:

```sh
printf 'omhl-commit-mark v1\n' > "$(git rev-parse --absolute-git-dir)/omhl-commit-mark"
```

The first line of the mark must be `omhl-commit-mark v1`. The mark lives in the git folder of this worktree, is never tracked, and the `post-commit` hook deletes it after the first commit on a branch that `.omhl/workflow.toml` declares as a `[[branch.exception]]` with `mode = "dry-run-with-mark"`. The mark of an earlier release, under another file name, is ignored, so write this one even if the old file exists. Until the plugin release reaches `main`, the installed command may still write the old name: write the mark by hand with the line above, at the path the `commit-mark-missing` hint shows.

The mark and a merge (premises.brn1-merge-leaves-mark-limit, closed, premises.brn1-accept-text-corrections): on a `dry-run-with-mark` exception branch, a local merge that git completes without conflict runs `pre-merge-commit`, which requires the mark, and then the `post-merge` script consumes it; a fast-forward also consumes it through `post-merge`; a `--squash` merge does not, because git passes the squash flag to `post-merge`, so the mark stays and frees the next direct commit. With a conflict, the final commit runs `pre-commit` and `post-commit` and consumes the mark. The earlier limit, where the mark was left after a merge, is closed.
