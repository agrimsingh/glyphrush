#!/usr/bin/env bash

baseline_repo_root() {
  if [[ -n "${GLYPHRUSH_BASELINE_ROOT:-}" ]]; then
    printf '%s\n' "$GLYPHRUSH_BASELINE_ROOT"
    return
  fi

  local script_dir
  script_dir="$(cd "$(dirname "${BASH_SOURCE[1]}")" && pwd)"
  cd "$script_dir/../.." && pwd
}

baseline_resolve_tool() {
  local explicit="$1"
  local name="$2"

  if [[ -n "$explicit" ]]; then
    printf '%s\n' "$explicit"
    return
  fi

  local root
  root="$(baseline_repo_root)"
  local candidates=(
    "$root/.glyphrush-baselines/node_modules/.bin/$name"
    "$root/.glyphrush-baselines/bin/$name"
  )

  local candidate
  for candidate in "${candidates[@]}"; do
    if [[ -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return
    fi
  done

  printf '%s\n' "$name"
}

baseline_resolve_python() {
  if [[ -n "${GLYPHRUSH_BASELINE_PYTHON:-}" ]]; then
    printf '%s\n' "$GLYPHRUSH_BASELINE_PYTHON"
    return
  fi

  local root
  root="$(baseline_repo_root)"
  local candidates=(
    "$root/.glyphrush-baselines/venv/bin/python3"
    "$root/.glyphrush-baselines/venv/bin/python"
    "$root/.venv/bin/python3"
    "$root/.venv/bin/python"
  )

  local candidate
  for candidate in "${candidates[@]}"; do
    if [[ -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return
    fi
  done

  printf '%s\n' "python3"
}

baseline_configure_tessdata() {
  if [[ -n "${TESSDATA_PREFIX:-}" ]]; then
    return
  fi

  local root
  root="$(baseline_repo_root)"
  local tessdata_dir="$root/.glyphrush-baselines/tessdata"

  if [[ -f "$tessdata_dir/eng.traineddata" ]]; then
    export TESSDATA_PREFIX="$tessdata_dir"
  fi
}
