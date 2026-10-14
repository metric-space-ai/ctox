# Origin: CTOX
# License: AGPL-3.0-only
# Shared validation for the exact Go module scope of an executed umbrella gate.
def safe_module_directory:
  type == "string" and
  (. == "." or
    (length > 0 and
     (startswith("/") | not) and
     (test("[\\\\\t\r\n\u0000]") | not) and
     (split("/") | all(. != "" and . != "." and . != ".."))));

def valid_pinned_go_scope:
  .schema == "ctox.cliproxyapi.pinned-go-modules.v1" and
  (.upstream_commit | test("^[0-9a-f]{40}$")) and
  (.modules | type) == "array" and (.modules | length) > 0 and
  (.modules | all(
    (.module | safe_module_directory) and
    .go_mod == (if .module == "." then "go.mod" else .module + "/go.mod" end))) and
  ([.modules[].module] | unique | length) == (.modules | length) and
  ([.modules[] | select(.module == ".")] | length) == 1;

def go_module_entries_match($scope):
  ($scope | valid_pinned_go_scope) and
  .upstream_commit == $scope.upstream_commit and
  ([.go_hydration[].module] | sort) == ([$scope.modules[].module] | sort) and
  ([.go_modules[].module] | sort) == ([$scope.modules[].module] | sort);

def hydrated_go_module_directories($raw):
  ($raw | split("\n") | map(select(length > 0) | split("\t"))) as $rows |
  if ($rows | length) > 0 and
     ($rows | all(length == 2 and
       (.[0] | test("^[0-9a-f]{64}$")) and
       (.[1] | type) == "string" and
       (.[1] == "go.mod" or .[1] == "go.sum" or
        (.[1] | endswith("/go.mod")) or (.[1] | endswith("/go.sum"))) and
       ((.[1] | sub("/?go\\.(mod|sum)$"; "")) as $parent |
        (if $parent == "" then "." else $parent end) | safe_module_directory))) and
     ([$rows[][1]] | unique | length) == ($rows | length)
  then [$rows[][1] | select(. == "go.mod" or endswith("/go.mod")) |
    if . == "go.mod" then "." else rtrimstr("/go.mod") end] | sort
  else error("invalid hydrated Go module manifest") end;
