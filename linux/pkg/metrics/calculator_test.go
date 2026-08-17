package metrics

import "testing"

func TestDecideGrowsBelowThreshold(t *testing.T) {
	policy := DefaultConfig()
	stats := MemoryStats{AvailableBytes: policy.LowFreeBytes - 1}
	state := VirtioMemState{RequestedBytes: policy.MinBytes, CurrentBytes: policy.MinBytes}
	decision, err := Decide(stats, state, policy)
	if err != nil {
		t.Fatal(err)
	}
	if !decision.Resize || decision.TargetBytes != policy.MinBytes+policy.StepBytes {
		t.Fatalf("got %+v, want one growth step", decision)
	}
}

func TestDecideShrinksAboveThreshold(t *testing.T) {
	policy := DefaultConfig()
	current := policy.MinBytes + policy.StepBytes
	decision, err := Decide(MemoryStats{AvailableBytes: policy.HighFreeBytes + 1}, VirtioMemState{RequestedBytes: current, CurrentBytes: current}, policy)
	if err != nil {
		t.Fatal(err)
	}
	if !decision.Resize || decision.TargetBytes != policy.MinBytes {
		t.Fatalf("got %+v, want minimum allocation", decision)
	}
}

func TestDecideWaitsForConvergence(t *testing.T) {
	policy := DefaultConfig()
	decision, err := Decide(MemoryStats{AvailableBytes: 0}, VirtioMemState{RequestedBytes: policy.MinBytes + policy.StepBytes, CurrentBytes: policy.MinBytes}, policy)
	if err != nil {
		t.Fatal(err)
	}
	if decision.Resize || decision.TargetBytes != policy.MinBytes+policy.StepBytes {
		t.Fatalf("got %+v, want no resize while converging", decision)
	}
}

func TestDecideClampsAtMaximum(t *testing.T) {
	policy := DefaultConfig()
	decision, err := Decide(MemoryStats{AvailableBytes: 0}, VirtioMemState{RequestedBytes: policy.MaxBytes, CurrentBytes: policy.MaxBytes}, policy)
	if err != nil {
		t.Fatal(err)
	}
	if decision.Resize || decision.TargetBytes != policy.MaxBytes {
		t.Fatalf("got %+v, want maximum clamp", decision)
	}
}
