package main

import (
	"context"
	"log"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/macg4dave/virtio-mem-controller/internal/config"
	"github.com/macg4dave/virtio-mem-controller/pkg/libvirt"
	"github.com/macg4dave/virtio-mem-controller/pkg/metrics"
	"github.com/macg4dave/virtio-mem-controller/pkg/qemu"
)

func main() {
	cfg, err := config.Parse(os.Args[1:])
	if err != nil {
		log.Fatal(err)
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	agent := qemu.CommandAgent{Domain: cfg.Domain}
	vm := libvirt.VirshClient{Domain: cfg.Domain, Alias: cfg.Alias}
	ticker := time.NewTicker(cfg.Interval)
	defer ticker.Stop()
	log.Printf("starting controller domain=%s interval=%s", cfg.Domain, cfg.Interval)

	poll := func() {
		stats, err := agent.MemoryStats(ctx)
		if err != nil {
			log.Printf("memory stats unavailable: %v", err)
			return
		}
		requested, current, err := vm.VirtioMemState(ctx)
		if err != nil {
			log.Printf("virtio-mem state unavailable: %v", err)
			return
		}
		decision, err := metrics.Decide(stats, metrics.VirtioMemState{RequestedBytes: requested, CurrentBytes: current}, cfg.Policy)
		if err != nil {
			log.Printf("policy error: %v", err)
			return
		}
		log.Printf("available=%d requested=%d current=%d target=%d reason=%s", stats.AvailableBytes, requested, current, decision.TargetBytes, decision.Reason)
		if decision.Resize {
			if err := vm.RequestMemory(ctx, decision.TargetBytes); err != nil {
				log.Printf("resize failed: %v", err)
			}
		}
	}

	poll()
	for {
		select {
		case <-ctx.Done():
			log.Println("controller stopped")
			return
		case <-ticker.C:
			poll()
		}
	}
}
