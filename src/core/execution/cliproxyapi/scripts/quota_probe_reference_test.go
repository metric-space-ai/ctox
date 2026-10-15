// Development-only Go overlay test: executes the exact frozen upstream probe.
// ref: internal/api/handlers/management/plugin_quota.go @ 0d18d3b365be000eeaee7de4615f9da596b2059d
// License: AGPL-3.0-only
package management

import (
	"encoding/json"
	"flag"
	"github.com/gin-gonic/gin"
	"github.com/router-for-me/CLIProxyAPI/v8/internal/config"
	coreauth "github.com/router-for-me/CLIProxyAPI/v8/sdk/cliproxy/auth"
	"net/http"
	"net/http/httptest"
	"os"
	"testing"
)

var quotaOracleInput = flag.String("ctox-quota-input", "", "isolated quota oracle input")
var quotaOracleOutput = flag.String("ctox-quota-output", "", "isolated quota oracle output")

type quotaProbeCase struct {
	Name     string          `json:"name"`
	Input    json.RawMessage `json:"input"`
	Mapping  map[string]any  `json:"mapping,omitempty"`
	Expected json.RawMessage `json:"expected,omitempty"`
	Reject   bool            `json:"reject,omitempty"`
}

func TestCTOXQuotaProbeOracle(t *testing.T) {
	if *quotaOracleInput == "" || *quotaOracleOutput == "" {
		t.Skip("explicit isolated fixture paths required")
	}
	raw, err := os.ReadFile(*quotaOracleInput)
	if err != nil {
		t.Fatal(err)
	}
	var cases []quotaProbeCase
	if err = json.Unmarshal(raw, &cases); err != nil {
		t.Fatal(err)
	}
	for i := range cases {
		item := &cases[i]
		upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.Header()["Date"] = nil; _, _ = w.Write(item.Input) }))
		h := NewHandlerWithoutConfigFilePath(&config.Config{AuthDir: t.TempDir()}, nil)
		ctx, _ := gin.CreateTestContext(httptest.NewRecorder())
		ctx.Request = httptest.NewRequest(http.MethodGet, "/probe", nil)
		probe := map[string]any{"url": upstream.URL}
		if item.Mapping != nil {
			probe["mapping"] = item.Mapping
		}
		response, handled, err := h.executeQuotaProbe(ctx, &coreauth.Auth{}, probe)
		upstream.Close()
		if !handled || err != nil {
			item.Reject = true
			continue
		}
		encoded, err := json.Marshal(response)
		if err != nil {
			item.Reject = true
			continue
		}
		item.Expected = encoded
	}
	out, err := json.MarshalIndent(cases, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	if err = os.WriteFile(*quotaOracleOutput, append(out, '\n'), 0600); err != nil {
		t.Fatal(err)
	}
}
