package qemu

import (
	"context"
	"encoding/json"
	"fmt"
	"os/exec"

	"github.com/macg4dave/virtio-mem-controller/pkg/metrics"
)

type Agent interface {
	MemoryStats(context.Context) (metrics.MemoryStats, error)
}

type CommandAgent struct {
	Domain string
	Virsh  string
}

type memoryResponse struct {
	Return []struct {
		Stat  string `json:"stat"`
		Value uint64 `json:"value"`
	} `json:"return"`
	Error *struct {
		Class string `json:"class"`
		Desc  string `json:"desc"`
	} `json:"error,omitempty"`
}

func ParseMemoryStats(data []byte) (metrics.MemoryStats, error) {
	var response memoryResponse
	if err := json.Unmarshal(data, &response); err != nil {
		return metrics.MemoryStats{}, fmt.Errorf("decode guest memory stats: %w", err)
	}
	if response.Error != nil {
		return metrics.MemoryStats{}, fmt.Errorf("guest agent %s: %s", response.Error.Class, response.Error.Desc)
	}
	var stats metrics.MemoryStats
	for _, item := range response.Return {
		switch item.Stat {
		case "stat-free":
			stats.FreeBytes = item.Value
		case "stat-available":
			stats.AvailableBytes = item.Value
		case "stat-total":
			stats.TotalBytes = item.Value
		}
	}
	if stats.AvailableBytes == 0 {
		stats.AvailableBytes = stats.FreeBytes
	}
	if stats.AvailableBytes == 0 {
		return metrics.MemoryStats{}, fmt.Errorf("guest agent response has no usable available memory")
	}
	return stats, nil
}

func (a CommandAgent) MemoryStats(ctx context.Context) (metrics.MemoryStats, error) {
	virsh := a.Virsh
	if virsh == "" {
		virsh = "virsh"
	}
	payload := `{"execute":"guest-get-memory-stats"}`
	output, err := exec.CommandContext(ctx, virsh, "qemu-agent-command", a.Domain, payload).Output()
	if err != nil {
		return metrics.MemoryStats{}, fmt.Errorf("query guest agent: %w", err)
	}
	return ParseMemoryStats(output)
}
