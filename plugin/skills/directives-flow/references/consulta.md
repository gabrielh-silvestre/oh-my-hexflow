# Querying the database

Index of the pages of the commands that read the project database. The rules that always apply are in the "Querying" section of [SKILL.md](../SKILL.md); here are the details that only a page resolves.

- A `get` of an id that does not exist is not an error: it says `node not found` (with `--format json`, `{"node":null}`) and exits 0. A malformed id is an error. Details in [get.md](get.md).
- The `--id-prefix` of `list` matches by whole segment and starts with a type namespace (`premises`, `work-items`, `checks`). Details in [list.md](list.md).
- A `--db` with a new path creates the database, outside the project folder; `--project-dir` must exist. This holds for all the commands below.

| Command | Page |
|---|---|
| Find nodes by type and filter | [list.md](list.md) |
| Read a node by id | [get.md](get.md) |
| Read the session context | [render.md](render.md) |
| Verify the hash chain | [verify.md](verify.md) |
| Regenerate the projection | [project.md](project.md) |
| Read the gaps report | [report-answers.md](report-answers.md) |
| Read the repeated premises report | [report-recurrences.md](report-recurrences.md) |
| List the sessions that recorded (`omhl report sessions`) | [report-sessions.md](report-sessions.md) |
| Common errors | [errors.md](errors.md) |
