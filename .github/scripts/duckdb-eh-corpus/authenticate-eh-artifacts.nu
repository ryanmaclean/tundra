#!/usr/bin/env nu
# Run in the compare job after all six artifacts have uploaded. gh must be authenticated.

def main [receipt_root: path, repo: string, run_id: int, head_sha: string] {
  if $repo != 'ryanmaclean/tundra' or $run_id <= 0 or $head_sha !~ '^[0-9a-f]{40}$' {
    error make {msg: 'invalid exact-run authentication inputs'}
  }
  let names = ['duckdb-eh-pair-image-default' 'duckdb-eh-pair-image-sounds'
    'duckdb-eh-pair-clean-default' 'duckdb-eh-pair-clean-sounds'
    'duckdb-eh-old-default' 'duckdb-eh-old-sounds']
  let run = (^gh api $"repos/($repo)/actions/runs/($run_id)" | from json)
  if $run.id != $run_id or $run.head_sha != $head_sha or $run.repository.full_name != $repo {
    error make {msg: 'workflow run identity/head mismatch'}
  }
  let listing = (^gh api $"repos/($repo)/actions/runs/($run_id)/artifacts?per_page=100" | from json)
  if $listing.total_count != 6 or ($listing.artifacts | length) != 6 {
    error make {msg: 'missing, extra or paginated exact-run artifacts'}
  }
  let listed_names = ($listing.artifacts | get name)
  if ($listed_names | uniq | length) != 6 or ($listed_names | sort) != ($names | sort) {
    error make {msg: 'wrong or duplicate exact-run artifact names'}
  }
  let zip_root = ($receipt_root | path join 'artifact-zips')
  mkdir $zip_root
  let python = r#'
import hashlib, json, pathlib, stat, sys, zipfile
archive, destination = sys.argv[1:]
root = pathlib.Path(destination)
members = {}
with zipfile.ZipFile(archive) as z:
    infos = z.infolist()
    if len(infos) > 128 or sum(i.file_size for i in infos) > 16 * 1024 * 1024:
        raise ValueError('artifact extraction exceeds bounded receipt cap')
    seen = set()
    for info in infos:
        name = info.filename
        path = pathlib.PurePosixPath(name)
        if (not name or '\\' in name or name.startswith('/') or
            any(p in ('', '.', '..') for p in name.rstrip('/').split('/')) or
            name in seen or path.is_absolute() or info.file_size < 0 or
            info.flag_bits & 0x1):
            raise ValueError('duplicate or unsafe ZIP member')
        seen.add(name)
        mode = (info.external_attr >> 16) & 0o170000
        if mode not in (0, stat.S_IFREG, stat.S_IFDIR):
            raise ValueError('special ZIP member')
        if info.is_dir() != (mode == stat.S_IFDIR) and mode != 0:
            raise ValueError('ZIP path/type mismatch')
    if not any(not i.is_dir() for i in infos):
        raise ValueError('empty artifact ZIP')
    root.mkdir(parents=True, exist_ok=True)
    total = 0
    for info in infos:
        if info.is_dir(): continue
        target = root.joinpath(*pathlib.PurePosixPath(info.filename).parts)
        target.parent.mkdir(parents=True, exist_ok=True)
        digest = hashlib.sha256()
        written = 0
        with z.open(info) as source, target.open('xb') as output:
            while True:
                chunk = source.read(65536)
                if not chunk: break
                written += len(chunk)
                total += len(chunk)
                if written > info.file_size or total > 16 * 1024 * 1024:
                    raise ValueError('actual ZIP expansion exceeds declared/bounded size')
                output.write(chunk)
                digest.update(chunk)
        if written != info.file_size:
            raise ValueError('declared/actual ZIP member size mismatch')
        members[info.filename] = digest.hexdigest()
print(json.dumps(members, sort_keys=True))
'#
  let ids = ($listing.artifacts | get id)
  if ($ids | uniq | length) != 6 or ($ids | any {|id| $id <= 0}) {
    error make {msg: 'duplicate or invalid artifact IDs'}
  }
  let rows = ($names | each {|name|
    let matches = ($listing.artifacts | where name == $name)
    if ($matches | length) != 1 { error make {msg: $"artifact lookup ambiguous: ($name)"} }
    let api = $matches.0
    if ($api.expired != false or $api.id <= 0 or
       $api.workflow_run.id != $run_id or $api.workflow_run.head_sha != $head_sha or
       $api.digest !~ '^sha256:[0-9a-f]{64}$' or
       $api.size_in_bytes <= 0 or $api.size_in_bytes > 16777216) {
      error make {msg: $"artifact API identity/digest invalid: ($name)"}
    }
    let zip_path = ($zip_root | path join $"($name).zip")
    ^gh api $"repos/($repo)/actions/artifacts/($api.id)/zip" o> $zip_path
    if $env.LAST_EXIT_CODE != 0 { error make {msg: $"artifact download failed: ($name)"} }
    let local_bytes = (ls $zip_path | get size.0 | into int)
    if $local_bytes != $api.size_in_bytes {
      error make {msg: $"artifact API/local ZIP length mismatch: ($name)"}
    }
    let zip_sha = (^sha256sum $zip_path | split row ' ' | first)
    if $api.digest != $"sha256:($zip_sha)" {
      error make {msg: $"artifact API/ZIP digest mismatch: ($name)"}
    }
    let destination = ($receipt_root | path join $name)
    if ($destination | path exists) { error make {msg: $"refusing existing extraction dir: ($destination)"} }
    let member_sha256 = (^python3 -c $python $zip_path $destination | from json)
    {name: $name, id: $api.id, run_id: $run_id, head_sha: $head_sha,
      api_digest: $api.digest, zip_sha256: $zip_sha,
      size_in_bytes: $api.size_in_bytes, member_sha256: $member_sha256}
  })
  {schema: 'tundra-eh-artifact-auth/v1', admission: 'NO', repo: $repo,
    run_id: $run_id, head_sha: $head_sha, artifacts: $rows} |
    to json --raw | save --force ($receipt_root | path join 'artifact-auth.json')
}
