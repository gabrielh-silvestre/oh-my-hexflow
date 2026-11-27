#!/usr/bin/env bash
# Mantém os grafos do GitNexus (completo e só produção) e do Graphify em dia com o checkout principal.
# Uso: bash scripts/graph-sync.sh sync [--docs]   (o modo `hook` é do .claude/settings.json)
set -euo pipefail

FULL_REPO="oh-my-hexflow"
PROD_REPO="oh-my-hexflow-prod"
# Mesmo recorte de testes do .graphifyignore.
TEST_PATH='^crates/[^/]+/tests/|(^|/)tests\.rs$|(^|/)tests/|(^|/)test_support(/|\.rs$)'
DOC_PATH='\.(md|mdx|txt|rst|html|yaml|yml)$'
PULL_COMMAND='\bgit\b(\s+-[Cc]\s+\S+|\s+--[[:alnum:]_-]+(=\S+)?)*\s+pull\b'
CLOSE_COMMAND='\bomhl\s+close\b'
SYNC_TOOLS=(gitnexus graphify jq rsync flock sha256sum)

export GITNEXUS_WORKER_POOL_SIZE="${GITNEXUS_WORKER_POOL_SIZE:-4}"
export GRAPHIFY_CLAUDE_CLI_MODEL=sonnet

messages=()

main_checkout() {
  local here common
  here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
  common=$(git -C "$here" rev-parse --path-format=absolute --git-common-dir) || return 1
  dirname "$common"
}

missing_tools() {
  local tool
  for tool in "${SYNC_TOOLS[@]}"; do
    command -v "$tool" >/dev/null 2>&1 || printf '%s ' "$tool"
  done
}

branch_refs() {
  git -C "$main" for-each-ref --format='%(refname) %(objectname)' refs/heads/main refs/heads/develop
}

visible_files() {
  git -C "$main" ls-files -z --cached --others --exclude-standard |
    while IFS= read -r -d '' path; do
      if [[ -f $main/$path ]]; then printf '%s\n' "$path"; fi
    done |
    LC_ALL=C sort -u
}

step() {
  local label=$1 out code=0
  shift
  out=$("$@" 2>&1) || code=$?
  if ((code != 0)); then
    messages+=("WARNING $label failed (exit $code):"$'\n'"$(tail -n 5 <<<"$out")")
  fi
  return "$code"
}

# Chamada dentro de `||`, onde o `set -e` não vale: cada comando propaga a própria falha.
mirror() {
  local target=$1 wanted=$1.wanted
  mkdir -p "$target" || return 1
  { grep -Ev "$TEST_PATH" "$files" || true; } >"$wanted" || return 1
  rsync -a --files-from="$wanted" "$main/" "$target/" || return 1
  (cd "$target" && find . -path ./.gitnexus -prune -o ! -type d -print) |
    sed 's|^\./||' | LC_ALL=C sort | LC_ALL=C comm -23 - "$wanted" |
    while IFS= read -r path; do
      rm -f -- "$target/$path" || exit 1
    done
}

check_leaks() {
  local out table leaked
  if ! out=$(gitnexus cypher 'MATCH (f:File) RETURN f.filePath' -r "$FULL_REPO" 2>/dev/null) ||
    ! table=$(jq -er '.markdown' <<<"$out" 2>/dev/null); then
    messages+=("WARNING could not check which files GitNexus indexed")
    return
  fi
  leaked=$(tail -n +3 <<<"$table" | sed -E 's/^\| *//; s/ *\|$//' | LC_ALL=C sort -u | LC_ALL=C comm -23 - "$files")
  if [[ -n $leaked ]]; then
    messages+=("WARNING GitNexus indexed $(wc -l <<<"$leaked") file(s) that git ignores; add them to .gitnexusignore: $(head -n 5 <<<"$leaked" | paste -sd ' ')")
  fi
}

sync_gitnexus() {
  local prod=$main/.ignore/graph-prod ok=0
  # Sem --force a análise incremental ignora edição sem commit e ainda responde sucesso.
  local analyze=(gitnexus analyze --force --index-only)
  if step "gitnexus full" "${analyze[@]}" --name "$FULL_REPO" "$main"; then
    check_leaks
  else
    ok=1
  fi
  if step "production mirror" mirror "$prod"; then
    step "gitnexus prod" "${analyze[@]}" --skip-git --name "$PROD_REPO" "$prod" || ok=1
  else
    ok=1
  fi
  return "$ok"
}

docs_digest() {
  { grep -E "$DOC_PATH" "$files" || true; } |
    { grep -Ev "$TEST_PATH|^\.claude/" || true; } |
    (cd "$main" && xargs -r -d '\n' sha256sum) |
    sha256sum | cut -d ' ' -f 1
}

sync_graphify() {
  local force_docs=$1 tags new_tags digest ok=0
  tags=$(git -C "$main" tag --list 'v*' | LC_ALL=C sort) || return 1
  new_tags=$(LC_ALL=C comm -23 <(sed '/^$/d' <<<"$tags") <(cat "$state/tags" 2>/dev/null || true))
  digest=$(docs_digest) || return 1
  if [[ ! -f $main/graphify-out/graph.json ]] && ! $force_docs; then
    messages+=("WARNING no Graphify graph yet; run: bash scripts/graph-sync.sh sync --docs")
    return 1
  fi

  if $force_docs || [[ -n $new_tags ]]; then
    step "graphify extract" graphify extract "$main" --backend claude-cli --max-workers 4 &&
      step "graphify cluster-only" graphify cluster-only "$main" --backend=claude-cli --model=sonnet ||
      return 1
    printf '%s\n' "$tags" | sed '/^$/d' >"$state/tags"
    printf '%s\n' "$digest" >"$state/docs"
    return 0
  fi

  step "graphify update" graphify update "$main" || ok=1
  if [[ $digest != "$(cat "$state/docs" 2>/dev/null || true)" ]]; then
    messages+=("NOTE Graphify documents are pending; they are extracted after the next release, or now with: bash scripts/graph-sync.sh sync --docs")
  fi
  return "$ok"
}

sync_graphs() {
  local force_docs=$1 name refs ok=true
  for name in .gitnexusignore .graphifyignore; do
    if [[ ! -f $main/$name ]]; then
      # Checkout sem a configuração (branch antiga, por exemplo) geraria grafos com o recorte errado.
      messages+=("WARNING sync skipped: $name not found in $main")
      return 0
    fi
  done

  state=$main/.ignore/graph-sync
  files=$state/files
  mkdir -p "$state"
  exec 9>"$state/lock"
  flock 9
  # Lista incompleta apagaria arquivos da cópia: sem ela, nada roda.
  if ! refs=$(branch_refs) || ! visible_files >"$files"; then
    messages+=("WARNING sync failed: could not list the files of $main")
    return 0
  fi
  sync_gitnexus || ok=false
  sync_graphify "$force_docs" || ok=false
  # Só marca os refs quando tudo passou, para o próximo pull tentar de novo.
  if $ok; then
    printf '%s\n' "$refs" >"$state/refs"
    messages+=("OK graphs synced from $main")
  fi
  flock -u 9
}

run_hook() {
  local command pulled=false closed=false
  [[ -z $(missing_tools) ]] || return 0
  command=$(jq -er '.tool_input.command | strings' 2>/dev/null) || return 0
  if grep -Eq "$PULL_COMMAND" <<<"$command"; then pulled=true; fi
  # A ajuda do close não fecha nada.
  if sed -E 's/\bomhl\s+close\b[^;&|]*\s(--help|-h)\b//g' <<<"$command" | grep -Eq "$CLOSE_COMMAND"; then closed=true; fi
  $pulled || $closed || return 0

  main=$(main_checkout) || return 0
  if ! $closed && [[ $(branch_refs) == "$(cat "$main/.ignore/graph-sync/refs" 2>/dev/null || true)" ]]; then
    return 0
  fi
  sync_graphs false
  jq -cn --arg context "graph-sync:"$'\n'"$(printf '%s\n' "${messages[@]}")" \
    '{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $context}}'
}

usage() {
  echo "usage: $0 sync [--docs] | hook" >&2
  exit 2
}

case "${1:-}" in
  hook)
    run_hook
    ;;
  sync)
    case "${2:-}" in
      "") force_docs=false ;;
      --docs) force_docs=true ;;
      *) usage ;;
    esac
    missing=$(missing_tools)
    if [[ -n $missing ]]; then
      echo "error: not installed: $missing" >&2
      exit 1
    fi
    main=$(main_checkout)
    sync_graphs "$force_docs"
    printf '%s\n' "${messages[@]}"
    if printf '%s\n' "${messages[@]}" | grep -q '^WARNING'; then exit 1; fi
    ;;
  *)
    usage
    ;;
esac
