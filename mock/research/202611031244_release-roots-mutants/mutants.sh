#!/usr/bin/env bash
# Mutation check for the release roots and the clone lookup. Each line of the table below is one mutant,
# five fields separated by " ;; ": a name, a file under mock/crates, a perl substitution written
# pattern@@replacement that must match exactly once, the cargo arguments naming the package, and the test
# filters. A mutant is killed when the tests fail with the change applied. A build failure and a pattern
# that does not match exactly once are reported apart, since they kill nothing. Run from the repository
# root, with the tree not edited meanwhile; every file is put back and touched, so cargo rebuilds it.
#
# Not in the table, because no test can tell it apart: the config being built once per process and not once
# per agent (the answer is the same either way). The first run of this table left the variable named in the
# text written when no root could be read standing, since the loader's own error for a missing file carries
# the path as well; the text is now asked for directly, in `tls_tests.rs`, and the three mutants of it below
# are killed by that.
set -u
CR=mock/crates
CORE="-p homma-core --lib"
ENGINE="-p homma-engine --bins"
killed=0; survived=0; broken=0
while IFS= read -r line; do
  [ -z "$line" ] && continue
  name=$(printf '%s' "$line" | awk -F' ;; ' '{print $1}')
  file=$(printf '%s' "$line" | awk -F' ;; ' '{print $2}')
  expr=$(printf '%s' "$line" | awk -F' ;; ' '{print $3}')
  pkg=$(printf '%s' "$line" | awk -F' ;; ' '{print $4}')
  filters=$(printf '%s' "$line" | awk -F' ;; ' '{print $5}')
  [ "$pkg" = CORE ] && pkg=$CORE
  [ "$pkg" = ENGINE ] && pkg=$ENGINE
  f="$CR/$file"
  cp "$f" "$f.orig"
  pat=${expr%%@@*}
  n=$(perl -0ne "print scalar(() = /${pat//\//\\/}/g)" "$f" 2>/dev/null || echo 0)
  perl -0pi -e "s#${pat}#${expr#*@@}#" "$f"
  if [ "$n" != 1 ] || cmp -s "$f" "$f.orig"; then
    echo "NO-MATCH  $name (matched ${n:-0} times)"; broken=$((broken+1))
  else
    out=$(cargo test --manifest-path mock/Cargo.toml $pkg -- $filters 2>&1)
    if echo "$out" | grep -qE '^error(\[E|: could not compile)'; then
      echo "BUILD     $name"; broken=$((broken+1))
    elif echo "$out" | grep -q 'test result: FAILED'; then
      echo "killed    $name"; killed=$((killed+1))
    else
      echo "SURVIVED  $name"; survived=$((survived+1))
    fi
  fi
  cp "$f.orig" "$f"; rm "$f.orig"; touch "$f"
done <<'TABLE'
the agent keeps ureq's own roots ;; homma-core/src/tls.rs ;; ureq::AgentBuilder::new\(\)\.tls_config\(CONFIG\.get_or_init\(client_config\)\.clone\(\)\)@@{ let _ = &CONFIG; ureq::AgentBuilder::new() } ;; CORE ;; tls registry whole_load
the root store reads nothing ;; homma-core/src/tls.rs ;; let \(added, _\) = store\.add_parsable_certificates\(loaded\.certs\);@@let added = 0usize; let _ = loaded.certs; ;; CORE ;; tls registry whole_load
a store with no root says nothing ;; homma-core/src/tls.rs ;; if added == 0 \{@@if false { ;; CORE ;; tls registry whole_load
a store with roots says it has none ;; homma-core/src/tls.rs ;; if added == 0 \{@@if true { ;; CORE ;; tls registry whole_load
the file the variable names is left out of the text ;; homma-core/src/tls.rs ;; \[\("SSL_CERT_FILE", file\), \("SSL_CERT_DIR", dir\)\]@@[("SSL_CERT_DIR", dir)] ;; CORE ;; tls registry whole_load
the directory the variable names is left out of the text ;; homma-core/src/tls.rs ;; \[\("SSL_CERT_FILE", file\), \("SSL_CERT_DIR", dir\)\]@@[("SSL_CERT_FILE", file)] ;; CORE ;; tls registry whole_load
the loader's errors are left out of the text ;; homma-core/src/tls.rs ;; for error in errors \{@@for error in errors.iter().take(0) { ;; CORE ;; tls registry whole_load
the github client builds its agent from nothing ;; homma-core/src/forge/github.rs ;; crate::tls::agent_builder\(\)@@ureq::AgentBuilder::new() ;; CORE ;; tls registry whole_load
the github client builds its agent with builder ;; homma-core/src/forge/github.rs ;; crate::tls::agent_builder\(\)@@ureq::builder() ;; CORE ;; tls registry whole_load
the forgejo client builds its agent from nothing ;; homma-core/src/forge/forgejo.rs ;; crate::tls::agent_builder\(\)@@ureq::AgentBuilder::new() ;; CORE ;; tls registry whole_load
the registry requests build their agent with builder ;; homma-core/src/release/registry.rs ;; crate::tls::agent_builder\(\)@@ureq::builder() ;; CORE ;; tls registry whole_load
the check allows builder ;; homma-core/src/tls_tests.rs ;; "json",\n\];@@"json",\n    "builder",\n]; ;; CORE ;; tls registry whole_load
the check exempts tls.rs by name ;; homma-core/src/tls_tests.rs ;; \.filter\(\|f\| f != &src\.join\("tls\.rs"\)\)@@.filter(|f| f.file_name() != Some(std::ffi::OsStr::new("tls.rs"))) ;; CORE ;; tls registry whole_load
the check lets any `ureq::Agent` outside the module through ;; homma-core/src/tls_tests.rs ;; "Request",@@"Agent",\n    "Request", ;; CORE ;; tls registry whole_load
the forgejo client names the agent type from `ureq` ;; homma-core/src/forge/forgejo.rs ;; agent:   crate::tls::Agent,@@agent:   ureq::Agent, ;; CORE ;; tls registry whole_load
the gate with no repository stays in the scope ;; homma-engine/src/cmd/release/mod.rs ;; \} => load_config\(cli\),@@} | ReleaseOp::Gate { repo: None, .. } => load_config(cli), ;; ENGINE ;; config_tests
the check reads comments ;; homma-core/src/tls_tests.rs ;; \.filter\(\|l\| !l\.trim_start\(\)\.starts_with\("//"\)\)@@.filter(|_| true) ;; CORE ;; tls registry whole_load
the fixture's local file names both clones, core ;; homma-core/src/testing.rs ;; repos = \[\\"inside\\"\]@@repos = [\\"inside\\", \\"outside\\"] ;; CORE ;; tls registry whole_load
the fixture's local file names both clones, engine ;; homma-core/src/testing.rs ;; repos = \[\\"inside\\"\]@@repos = [\\"inside\\", \\"outside\\"] ;; ENGINE ;; config_tests
no verb loads the whole workspace ;; homma-engine/src/cmd/release/mod.rs ;; _ => load_config_whole\(cli\),@@_ => load_config(cli), ;; ENGINE ;; config_tests
the sweep loads the whole workspace ;; homma-engine/src/cmd/release/mod.rs ;; \} => load_config\(cli\),@@} => load_config_whole(cli), ;; ENGINE ;; config_tests
the lookup ignores the directory it is given ;; homma-engine/src/cmd/util.rs ;; let here = here\.canonicalize\(\)\.unwrap_or_else\(\|_\| here\.to_path_buf\(\)\);@@let here = std::env::current_dir()?; ;; ENGINE ;; config_tests
TABLE
echo "killed=$killed survived=$survived other=$broken"
