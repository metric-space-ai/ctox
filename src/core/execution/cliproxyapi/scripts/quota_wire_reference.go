// Development oracle only. Run from the frozen upstream Go module.
// ref: sdk/pluginapi/types.go:1577-1769 @ 0d18d3b365be000eeaee7de4615f9da596b2059d
// License: AGPL-3.0-only
package main

import (
	"encoding/json"
	"fmt"
	pluginapi "github.com/router-for-me/CLIProxyAPI/v8/sdk/pluginapi"
	"golang.org/x/text/currency"
	"os"
)

type quotaCase struct {
	Name     string          `json:"name"`
	Kind     string          `json:"kind"`
	Input    json.RawMessage `json:"input"`
	Expected json.RawMessage `json:"expected,omitempty"`
	Reject   bool            `json:"reject,omitempty"`
}

func main() {
	if len(os.Args) == 2 && os.Args[1] == "currency" {
		var codes []string
		for a := 'A'; a <= 'Z'; a++ {
			for b := 'A'; b <= 'Z'; b++ {
				for c := 'A'; c <= 'Z'; c++ {
					code := string([]rune{a, b, c})
					if _, err := currency.ParseISO(code); err == nil {
						codes = append(codes, code)
					}
				}
			}
		}
		if err := json.NewEncoder(os.Stdout).Encode(codes); err != nil {
			panic(err)
		}
		return
	}
	var cases []quotaCase
	if err := json.NewDecoder(os.Stdin).Decode(&cases); err != nil {
		panic(err)
	}
	for i := range cases {
		var value any
		switch cases[i].Kind {
		case "fetch":
			value = &pluginapi.QuotaFetchResponse{}
		case "bucket":
			value = &pluginapi.QuotaBucket{}
		case "metric":
			value = &pluginapi.QuotaMetric{}
		case "describe":
			value = &pluginapi.QuotaDescribeResponse{}
		case "reset":
			value = &pluginapi.QuotaResetResponse{}
		default:
			panic(fmt.Sprintf("unknown quota kind %q", cases[i].Kind))
		}
		if err := json.Unmarshal(cases[i].Input, value); err != nil {
			cases[i].Reject = true
			continue
		}
		out, err := json.Marshal(value)
		if err != nil {
			cases[i].Reject = true
			continue
		}
		cases[i].Expected = out
	}
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	if err := enc.Encode(cases); err != nil {
		panic(err)
	}
}
