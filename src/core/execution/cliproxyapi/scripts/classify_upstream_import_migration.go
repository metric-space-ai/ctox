// Origin: CTOX
// License: AGPL-3.0-only
// Development-only review aid. It never accepts a review or changes the upstream pin.
package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"go/parser"
	"go/token"
	"os"
	"os/exec"
	"strconv"
	"strings"
)

type fileReview struct {
	Path             string `json:"path"`
	BeforeBlob       string `json:"before_blob"`
	AfterBlob        string `json:"after_blob"`
	Classification   string `json:"classification"`
	Reason           string `json:"reason,omitempty"`
	ImportsRewritten int    `json:"imports_rewritten"`
	BeforeSHA256     string `json:"before_sha256,omitempty"`
	AfterSHA256      string `json:"after_sha256,omitempty"`
	NormalizedSHA256 string `json:"normalized_sha256,omitempty"`
}

type migrationReport struct {
	Schema                 string       `json:"schema"`
	Base                   string       `json:"base_commit"`
	Candidate              string       `json:"candidate_commit"`
	ModuleBefore           string       `json:"module_before"`
	ModuleAfter            string       `json:"module_after"`
	ModifiedGoFiles        int          `json:"modified_go_files"`
	ImportOnlyCandidates   int          `json:"import_only_candidates"`
	OtherChanges           int          `json:"other_changes"`
	SemanticReviewComplete bool         `json:"semantic_review_complete"`
	PinPromoted            bool         `json:"pin_promoted"`
	Files                  []fileReview `json:"files"`
}

func gitOutput(repo string, args ...string) ([]byte, error) {
	argv := append([]string{"-C", repo, "--literal-pathspecs"}, args...)
	output, err := exec.Command("git", argv...).Output()
	if err != nil {
		return nil, fmt.Errorf("git %s: %w", args[0], err)
	}
	return output, nil
}

func validateRevision(ref string) error {
	if len(ref) != 40 {
		return errors.New("expected a full immutable 40-character commit hash")
	}
	_, err := hex.DecodeString(ref)
	return err
}

func readBlob(repo, ref, path string) ([]byte, error) {
	object := ref + ":" + path
	sizeRaw, err := gitOutput(repo, "cat-file", "-s", object)
	if err != nil {
		return nil, err
	}
	size, err := strconv.ParseInt(strings.TrimSpace(string(sizeRaw)), 10, 64)
	if err != nil || size < 0 || size > 8*1024*1024 {
		return nil, errors.New("source blob exceeds the 8 MiB review limit")
	}
	data, err := gitOutput(repo, "show", object)
	if err != nil {
		return nil, err
	}
	if int64(len(data)) != size {
		return nil, errors.New("source blob size changed")
	}
	return data, nil
}

func treeEntry(repo, ref, path string) (mode, blob string, err error) {
	data, err := gitOutput(repo, "ls-tree", "-z", ref, "--", path)
	if err != nil {
		return "", "", err
	}
	if bytes.Count(data, []byte{0}) != 1 {
		return "", "", errors.New("expected exactly one tree entry")
	}
	fields := strings.Fields(strings.SplitN(string(data), "\t", 2)[0])
	if len(fields) != 3 || fields[1] != "blob" {
		return "", "", errors.New("expected a source blob")
	}
	return fields[0], fields[2], nil
}

func modulePath(source []byte) (string, error) {
	for _, line := range strings.Split(string(source), "\n") {
		fields := strings.Fields(strings.SplitN(line, "//", 2)[0])
		if len(fields) == 0 {
			continue
		}
		if len(fields) != 2 || fields[0] != "module" {
			return "", errors.New("unsupported go.mod header; expected module declaration first")
		}
		path := fields[1]
		if strings.HasPrefix(path, "\"") {
			decoded, err := strconv.Unquote(path)
			if err != nil {
				return "", err
			}
			path = decoded
		}
		if path == "" {
			return "", errors.New("empty module path")
		}
		return path, nil
	}
	return "", errors.New("missing module declaration")
}

// Rewrite only parser-identified ImportSpec path tokens. Every other byte, including
// aliases, comments, spacing, runtime strings and the complete function bodies, survives.
func normalizeImports(source []byte, oldModule, newModule string) ([]byte, int, error) {
	if oldModule == "" || newModule == "" || oldModule == newModule {
		return nil, 0, errors.New("expected distinct nonempty module paths")
	}
	positions := token.NewFileSet()
	file, err := parser.ParseFile(positions, "source.go", source, parser.ImportsOnly)
	if err != nil {
		return nil, 0, err
	}
	normalized := bytes.Clone(source)
	rewritten := 0
	for i := len(file.Imports) - 1; i >= 0; i-- {
		literal := file.Imports[i].Path
		path, err := strconv.Unquote(literal.Value)
		if err != nil {
			return nil, 0, err
		}
		if path != newModule && !strings.HasPrefix(path, newModule+"/") {
			continue
		}
		quote := literal.Value[:1]
		if literal.Value != quote+path+quote {
			return nil, 0, errors.New("escaped import path requires manual review")
		}
		replacement := []byte(quote + oldModule + strings.TrimPrefix(path, newModule) + quote)
		start, end := positions.Position(literal.Pos()).Offset, positions.Position(literal.End()).Offset
		if start < 0 || end > len(source) || string(source[start:end]) != literal.Value {
			return nil, 0, errors.New("invalid import token offsets")
		}
		normalized = append(append(append([]byte{}, normalized[:start]...), replacement...), normalized[end:]...)
		rewritten++
	}
	return normalized, rewritten, nil
}

func digest(data []byte) string { sum := sha256.Sum256(data); return hex.EncodeToString(sum[:]) }

func analyze(repo, base, candidate string) (migrationReport, error) {
	report := migrationReport{Schema: "ctox.cliproxyapi.import-migration-review.v1", Base: base, Candidate: candidate, Files: []fileReview{}}
	if err := validateRevision(base); err != nil {
		return report, err
	}
	if err := validateRevision(candidate); err != nil {
		return report, err
	}
	oldMod, err := readBlob(repo, base, "go.mod")
	if err != nil {
		return report, err
	}
	newMod, err := readBlob(repo, candidate, "go.mod")
	if err != nil {
		return report, err
	}
	report.ModuleBefore, err = modulePath(oldMod)
	if err != nil {
		return report, err
	}
	report.ModuleAfter, err = modulePath(newMod)
	if err != nil {
		return report, err
	}
	if report.ModuleBefore == report.ModuleAfter {
		return report, errors.New("no module migration to classify")
	}
	names, err := gitOutput(repo, "diff", "--name-only", "--diff-filter=M", "-z", base, candidate, "--")
	if err != nil {
		return report, err
	}
	paths := bytes.Split(bytes.TrimSuffix(names, []byte{0}), []byte{0})
	if len(paths) > 5000 {
		return report, errors.New("modified path count exceeds review limit")
	}
	for _, rawPath := range paths {
		path := string(rawPath)
		if !strings.HasSuffix(path, ".go") {
			continue
		}
		result := fileReview{Path: path, Classification: "other_change"}
		oldMode, oldBlob, err := treeEntry(repo, base, path)
		if err != nil {
			return report, err
		}
		newMode, newBlob, err := treeEntry(repo, candidate, path)
		if err != nil {
			return report, err
		}
		result.BeforeBlob, result.AfterBlob = oldBlob, newBlob
		if oldMode != newMode || (newMode != "100644" && newMode != "100755") {
			result.Reason = "file mode or file type changed"
		} else {
			before, err := readBlob(repo, base, path)
			if err != nil {
				return report, err
			}
			after, err := readBlob(repo, candidate, path)
			if err != nil {
				return report, err
			}
			result.BeforeSHA256, result.AfterSHA256 = digest(before), digest(after)
			normalized, count, err := normalizeImports(after, report.ModuleBefore, report.ModuleAfter)
			if err != nil {
				result.Reason = "import parse requires manual review"
			} else {
				result.ImportsRewritten, result.NormalizedSHA256 = count, digest(normalized)
				if count > 0 && bytes.Equal(before, normalized) {
					result.Classification = "import_only_candidate"
				} else {
					result.Reason = "bytes differ outside the migrated import path tokens"
				}
			}
		}
		report.Files = append(report.Files, result)
		report.ModifiedGoFiles++
		if result.Classification == "import_only_candidate" {
			report.ImportOnlyCandidates++
		} else {
			report.OtherChanges++
		}
	}
	return report, nil
}

func main() {
	repo := flag.String("repo", "", "local immutable upstream Git checkout")
	base := flag.String("base", "", "full accepted commit hash")
	candidate := flag.String("candidate", "", "full candidate commit hash")
	output := flag.String("out", "", "new report file; stdout when empty")
	flag.Parse()
	report, err := analyze(*repo, *base, *candidate)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	writer := os.Stdout
	if *output != "" {
		writer, err = os.OpenFile(*output, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0600)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		defer writer.Close()
	}
	encoder := json.NewEncoder(writer)
	encoder.SetIndent("", "  ")
	if err := encoder.Encode(report); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
