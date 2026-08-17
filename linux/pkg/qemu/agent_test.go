package qemu

import "testing"

func TestParseMemoryStats(t *testing.T) {
	data := []byte(`{"return":[{"stat":"stat-free","value":2147483648},{"stat":"stat-available","value":3221225472},{"stat":"stat-total","value":8589934592}]}`)
	stats, err := ParseMemoryStats(data)
	if err != nil {
		t.Fatal(err)
	}
	if stats.FreeBytes != 2147483648 || stats.AvailableBytes != 3221225472 || stats.TotalBytes != 8589934592 {
		t.Fatalf("unexpected stats: %+v", stats)
	}
}

func TestParseMemoryStatsFallsBackToFree(t *testing.T) {
	stats, err := ParseMemoryStats([]byte(`{"return":[{"stat":"stat-free","value":123}]}`))
	if err != nil {
		t.Fatal(err)
	}
	if stats.AvailableBytes != 123 {
		t.Fatalf("available=%d, want free fallback", stats.AvailableBytes)
	}
}

func TestParseMemoryStatsReportsAgentError(t *testing.T) {
	_, err := ParseMemoryStats([]byte(`{"error":{"class":"CommandNotFound","desc":"unsupported"}}`))
	if err == nil {
		t.Fatal("expected guest-agent error")
	}
}
