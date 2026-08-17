package config

import "testing"

func TestParseDefaults(t *testing.T) {
	cfg, err := Parse(nil)
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Domain != "win11_gpu" || cfg.Interval.Seconds() != 10 {
		t.Fatalf("unexpected config: %+v", cfg)
	}
}

func TestParseRejectsInvalidRange(t *testing.T) {
	if _, err := Parse([]string{"--min-memory", "28", "--max-memory", "8"}); err == nil {
		t.Fatal("expected invalid range")
	}
}
