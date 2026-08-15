Features eBPF for Linux >= 5.8

Precise Disk I/O and Latency Profiling 
Problem Solved: Identifying the specific process choking the hard drive or generating I/O wait times (iowait). 
eBPF Feature: Attaching tracepoints to block I/O operations (`block_rq_issue` and `block_rq_complete`). Benefit: Calculating actual read/write latency per application or file; proactive detection of disk saturation or storage degradation before failure occurs.

Granular network observability (L3/L4)
Problem solved: Lack of visibility into which service is consuming bandwidth or experiencing intermittent network outages. eBPF functionality: Interception via kprobe/tcp_connect, kprobe/tcp_retransmit, or eBPF sockets. Benefits: Measurement of TCP handshake times (RTT/Round-Trip Time) and retransmission rates per process/container. Real-time detection of DNS resolution failures or refused network connections. Automatic mapping of network dependencies between services.

Critical event capture (OOM Killer & Panics)
Problem addressed: When a process is terminated due to an Out-Of-Memory (OOM) condition, the system log often provides the agent with little immediate context. eBPF feature: Hooking into `mark_victim` / `oom_kill_process`. Benefit: Captures a snapshot of the exact context during an OOM crash (precise memory usage, offending processes at that specific moment) and triggers a high-priority alert.
