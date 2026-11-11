---
description: Create the omhl develop mark so the next commit on develop can run the before-commit checks
---

Create the develop mark of this checkout. Run exactly:

```sh
printf 'omhl-develop-mark v1\n' > "$(git rev-parse --absolute-git-dir)/omhl-develop-mark"
```

The first line of the mark must be `omhl-develop-mark v1`. The mark lives in the git folder of this worktree, is never tracked, and the `post-commit` hook deletes it after the first commit made on `develop`.
