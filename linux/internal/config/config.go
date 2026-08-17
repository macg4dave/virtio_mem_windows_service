package config

import (
	"flag"
	"time"

	"github.com/macg4dave/virtio-mem-controller/pkg/metrics"
)

type Config struct {
	Domain   string
	Alias    string
	Interval time.Duration
	Policy   metrics.Config
}

func Parse(args []string) (Config, error) {
	defaults := metrics.DefaultConfig()
	flags := flag.NewFlagSet("virtio-mem-controller", flag.ContinueOnError)
	var cfg Config
	flags.StringVar(&cfg.Domain, "domain", "win11_gpu", "libvirt domain name")
	flags.StringVar(&cfg.Alias, "alias", "ua-virtiomem0", "virtio-mem device alias")
	flags.DurationVar(&cfg.Interval, "interval", 10*time.Second, "poll interval")
	minGB := flags.Uint64("min-memory", 8, "minimum memory in GiB")
	maxGB := flags.Uint64("max-memory", 28, "maximum memory in GiB")
	stepGB := flags.Uint64("step-memory", 2, "resize step in GiB")
	if err := flags.Parse(args); err != nil {
		return Config{}, err
	}
	const gib = uint64(1024 * 1024 * 1024)
	cfg.Policy = defaults
	cfg.Policy.MinBytes = *minGB * gib
	cfg.Policy.MaxBytes = *maxGB * gib
	cfg.Policy.StepBytes = *stepGB * gib
	return cfg, cfg.Policy.Validate()
}
