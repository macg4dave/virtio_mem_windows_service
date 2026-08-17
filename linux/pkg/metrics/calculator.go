package metrics

import "fmt"

// Config controls the conservative virtio-mem policy.
type Config struct {
	MinBytes      uint64
	MaxBytes      uint64
	StepBytes     uint64
	LowFreeBytes  uint64
	HighFreeBytes uint64
}

func DefaultConfig() Config {
	const gib = uint64(1024 * 1024 * 1024)
	return Config{MinBytes: 8 * gib, MaxBytes: 28 * gib, StepBytes: 2 * gib, LowFreeBytes: 2 * gib, HighFreeBytes: 6 * gib}
}

type MemoryStats struct {
	FreeBytes      uint64
	AvailableBytes uint64
	TotalBytes     uint64
}

type VirtioMemState struct {
	RequestedBytes uint64
	CurrentBytes   uint64
}

type Decision struct {
	TargetBytes uint64
	Resize      bool
	Reason      string
}

func (c Config) Validate() error {
	if c.MinBytes == 0 || c.MaxBytes < c.MinBytes || c.StepBytes == 0 || c.LowFreeBytes >= c.HighFreeBytes {
		return fmt.Errorf("invalid memory policy")
	}
	if (c.MaxBytes-c.MinBytes)%c.StepBytes != 0 {
		return fmt.Errorf("memory range must be divisible by step")
	}
	return nil
}

// Decide never issues a second request while virtio-mem is converging.
func Decide(stats MemoryStats, state VirtioMemState, c Config) (Decision, error) {
	if err := c.Validate(); err != nil {
		return Decision{}, err
	}
	if state.CurrentBytes != state.RequestedBytes {
		return Decision{TargetBytes: state.RequestedBytes, Reason: "waiting for virtio-mem convergence"}, nil
	}

	target := state.CurrentBytes
	reason := "within hysteresis thresholds"
	if stats.AvailableBytes < c.LowFreeBytes {
		target += c.StepBytes
		reason = "available memory below lower threshold"
	} else if stats.AvailableBytes > c.HighFreeBytes {
		if target > c.StepBytes {
			target -= c.StepBytes
		} else {
			target = 0
		}
		reason = "available memory above upper threshold"
	}
	if target < c.MinBytes {
		target = c.MinBytes
	}
	if target > c.MaxBytes {
		target = c.MaxBytes
	}
	return Decision{TargetBytes: target, Resize: target != state.CurrentBytes, Reason: reason}, nil
}
