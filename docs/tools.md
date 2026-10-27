cargo xtask metrics --config deploy/config_files/host_list.toml

powershell -ExecutionPolicy Bypass -File C:\Users\Dave\Watch-Ram.ps1

cargo xtask watch --config deploy/config_files/host_list.toml

Metrics loads the host list and its sibling settings files. It reads the built-in
guest control publication over QGA; optional telemetry paths can be omitted.

`Watch-Ram.ps1` reads Windows memory directly with `GetPerformanceInfo`. It
does not load configuration or require the service or telemetry output.
