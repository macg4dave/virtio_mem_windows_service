package libvirt

import (
	"context"
	"encoding/xml"
	"fmt"
	"os/exec"
	"strconv"
	"strings"
)

// Client is intentionally small so policy tests do not require a live VM.
type Client interface {
	VirtioMemState(context.Context) (requested, current uint64, err error)
	RequestMemory(context.Context, uint64) error
}

type VirshClient struct {
	Domain string
	Alias  string
	Virsh  string
}

type domainXML struct {
	Memory []memoryDevice `xml:"memory"`
}
type memoryDevice struct {
	Model string    `xml:"model,attr"`
	Any   []xmlNode `xml:",any"`
}
type xmlNode struct {
	XMLName xml.Name
	Unit    string    `xml:"unit,attr"`
	Value   string    `xml:",chardata"`
	Any     []xmlNode `xml:",any"`
}

func (v VirshClient) command(ctx context.Context, args ...string) ([]byte, error) {
	binary := v.Virsh
	if binary == "" {
		binary = "virsh"
	}
	output, err := exec.CommandContext(ctx, binary, args...).Output()
	if err != nil {
		return nil, fmt.Errorf("virsh %v: %w", args, err)
	}
	return output, nil
}

func (v VirshClient) VirtioMemState(ctx context.Context) (uint64, uint64, error) {
	data, err := v.command(ctx, "dumpxml", v.Domain)
	if err != nil {
		return 0, 0, err
	}
	var domain domainXML
	if err := xml.Unmarshal(data, &domain); err != nil {
		return 0, 0, fmt.Errorf("decode domain XML: %w", err)
	}
	for _, device := range domain.Memory {
		if device.Model != "virtio-mem" {
			continue
		}
		values := map[string]uint64{}
		for _, node := range device.Any {
			collect(node, values)
		}
		requested, okRequested := values["requested"]
		current, okCurrent := values["current"]
		if okRequested && okCurrent {
			return requested, current, nil
		}
	}
	return 0, 0, fmt.Errorf("virtio-mem requested/current values not found")
}

func collect(node xmlNode, values map[string]uint64) {
	if node.XMLName.Local == "requested" || node.XMLName.Local == "current" {
		if value, err := strconv.ParseUint(strings.TrimSpace(node.Value), 10, 64); err == nil {
			if node.Unit == "GiB" || node.Unit == "G" {
				value *= 1024 * 1024 * 1024
			} else if node.Unit == "MiB" || node.Unit == "M" {
				value *= 1024 * 1024
			} else if node.Unit == "KiB" || node.Unit == "K" {
				value *= 1024
			}
			values[node.XMLName.Local] = value
		}
	}
	for _, child := range node.Any {
		collect(child, values)
	}
}

func (v VirshClient) RequestMemory(ctx context.Context, bytes uint64) error {
	if bytes == 0 {
		return fmt.Errorf("requested memory must be positive")
	}
	_, err := v.command(ctx, "update-memory-device", v.Domain, "--alias", v.Alias, "--requested-size", fmt.Sprintf("%dB", bytes), "--live")
	return err
}
