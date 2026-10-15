// Origin: CTOX
// License: AGPL-3.0-only
package main

import (
	"bytes"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestNormalizeImportsCannotHideOtherChanges(t *testing.T) {
	const oldModule = "example.invalid/proxy/v7"
	const newModule = "example.invalid/proxy/v8"
	grouped := "// Grüße\npackage p\nimport (\n a \"" + oldModule + "/a\"\n _ `" + oldModule + "/b`\n)\nvar value = 1\n"
	cases := []struct {
		name, before, after   string
		equivalent, wantError bool
	}{
		{"aliases raw quotes and unicode offsets", grouped, strings.ReplaceAll(grouped, oldModule, newModule), true, false},
		{"behavior changes", grouped, strings.ReplaceAll(strings.ReplaceAll(grouped, oldModule, newModule), "value = 1", "value = 2"), false, false},
		{"alias changes", grouped, strings.ReplaceAll(strings.ReplaceAll(grouped, oldModule, newModule), "a \"", "other \""), false, false},
		{"comments are evidence", "// " + oldModule + "\n" + grouped, "// " + newModule + "\n" + strings.ReplaceAll(grouped, oldModule, newModule), false, false},
		{"runtime string is not an import", "package p\nvar path = \"" + oldModule + "/a\"\n", "package p\nvar path = \"" + newModule + "/a\"\n", false, false},
		{"prefix collision", "package p\nimport \"" + oldModule + "0/a\"\n", "package p\nimport \"" + newModule + "0/a\"\n", false, false},
		{"whitespace changes", grouped, strings.ReplaceAll(grouped, oldModule, newModule) + "\n", false, false},
		{"unchanged source", grouped, grouped, false, false},
		{"escaped path requires review", "package p\nimport \"" + oldModule + "/a\"\n", "package p\nimport \"example.invalid/proxy/v\\x38/a\"\n", false, true},
		{"malformed import", grouped, "package p\nimport \"unterminated", false, true},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			normalized, count, err := normalizeImports([]byte(tc.after), oldModule, newModule)
			if (err != nil) != tc.wantError {
				t.Fatalf("error=%v, want error=%v", err, tc.wantError)
			}
			equivalent := err == nil && count > 0 && bytes.Equal([]byte(tc.before), normalized)
			if equivalent != tc.equivalent {
				t.Fatalf("classified equivalence=%v, want=%v; rewrites=%d", equivalent, tc.equivalent, count)
			}
		})
	}
}

func TestModuleHeaderIsExplicit(t *testing.T) {
	for _, input := range []string{"", "go 1.26.0\nmodule example.invalid/proxy\n", "module", "module a b"} {
		if _, err := modulePath([]byte(input)); err == nil {
			t.Fatalf("accepted unsupported header %q", input)
		}
	}
	got, err := modulePath([]byte("// retained comment\nmodule \"example.invalid/proxy/v8\"\n\ngo 1.26.0\n"))
	if err != nil || got != "example.invalid/proxy/v8" {
		t.Fatalf("module=%q error=%v", got, err)
	}
	if _, _, err := normalizeImports([]byte("package p"), "same", "same"); err == nil {
		t.Fatal("accepted unchanged module namespace")
	}
}

func TestGitReportRetainsBehaviorAndModeChanges(t *testing.T) {
	repo := t.TempDir()
	git := func(args ...string) string {
		argv := append([]string{"-c", "core.hooksPath=/dev/null", "-c", "commit.gpgSign=false", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-C", repo}, args...)
		out, err := exec.Command("git", argv...).CombinedOutput()
		if err != nil {
			t.Fatalf("git %v: %v: %s", args, err, out)
		}
		return strings.TrimSpace(string(out))
	}
	write := func(path, data string) {
		if err := os.WriteFile(filepath.Join(repo, path), []byte(data), 0600); err != nil {
			t.Fatal(err)
		}
	}
	git("init", "-q")
	source := "package p\nimport \"example.invalid/proxy/v7/a\"\nvar value = 1\n"
	write("go.mod", "module example.invalid/proxy/v7\n")
	for _, name := range []string{"pure.go", "behavior.go", "mode.go", "[literal].go"} {
		write(name, source)
	}
	git("add", ".")
	git("commit", "-qm", "base")
	base := git("rev-parse", "HEAD")
	write("go.mod", "module example.invalid/proxy/v8\n")
	migrated := strings.ReplaceAll(source, "/v7/", "/v8/")
	for _, name := range []string{"pure.go", "behavior.go", "mode.go", "[literal].go"} {
		write(name, migrated)
	}
	write("behavior.go", strings.ReplaceAll(migrated, "value = 1", "value = 2"))
	git("add", ".")
	git("update-index", "--chmod=+x", "mode.go")
	git("commit", "-qm", "candidate")
	candidate := git("rev-parse", "HEAD")
	report, err := analyze(repo, base, candidate)
	if err != nil {
		t.Fatal(err)
	}
	if report.ModifiedGoFiles != 4 || report.ImportOnlyCandidates != 2 || report.OtherChanges != 2 {
		t.Fatalf("unexpected classifications: %+v", report)
	}
	if report.SemanticReviewComplete || report.PinPromoted {
		t.Fatal("classification must never accept the review or promote the pin")
	}
	for _, file := range report.Files {
		if file.Classification == "import_only_candidate" && file.NormalizedSHA256 != file.BeforeSHA256 {
			t.Fatalf("missing byte equality for %s", file.Path)
		}
		if file.Path == "mode.go" && file.Classification == "import_only_candidate" {
			t.Fatal("mode change was masked")
		}
	}
	if _, err := analyze(repo, "HEAD", candidate); err == nil {
		t.Fatal("mutable revision accepted")
	}
}
