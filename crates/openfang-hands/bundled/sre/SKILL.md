---
name: sre-hand-skill
version: "1.0.0"
description: "Expert knowledge for Site Reliability Engineering — SLO/SLI methodology, incident response, cloud infrastructure patterns, cost optimization, and observability across AWS and Google Cloud"
runtime: prompt_only
---

# Site Reliability Engineering Expert Knowledge

## SRE Foundations (Google SRE Book Principles)

### Core Tenets
1. **Reliability is the most important feature** — users can't use features if the service is down
2. **Error budgets** — balance reliability with velocity; spend the budget on innovation
3. **Toil elimination** — automate repetitive operational work; if a human does it twice, script it
4. **Monitoring is not alerting** — collect everything, alert on symptoms not causes
5. **Blameless postmortems** — focus on systems, not individuals

### The Four Golden Signals
| Signal | What It Measures | Example Metrics |
|--------|-----------------|-----------------|
| **Latency** | Time to serve a request | p50, p95, p99 response time |
| **Traffic** | Demand on the system | Requests/sec, concurrent connections |
| **Errors** | Rate of failed requests | 5xx rate, error ratio, timeout rate |
| **Saturation** | How full the system is | CPU%, memory%, disk%, queue depth |

### USE Method (Brendan Gregg)
For every resource (CPU, memory, disk, network):
- **Utilization**: Percentage of time the resource is busy
- **Saturation**: Degree to which the resource has extra work queued
- **Errors**: Count of error events

### RED Method (Tom Wilkie)
For every service:
- **Rate**: Requests per second
- **Errors**: Number of failed requests per second
- **Duration**: Distribution of request durations (histograms)

---

## SLO/SLI/SLA Framework

### Definitions
- **SLI** (Service Level Indicator): A quantitative measure of service behavior (e.g., request latency p99)
- **SLO** (Service Level Objective): A target value for an SLI (e.g., p99 latency < 200ms)
- **SLA** (Service Level Agreement): A contract with consequences for missing SLOs
- **Error Budget**: The acceptable amount of unreliability (1 - SLO target)

### Common SLIs by Service Type
| Service Type | Availability SLI | Latency SLI | Quality SLI |
|-------------|-----------------|-------------|-------------|
| HTTP API | successful requests / total | p99 response time | correct responses / total |
| Batch job | successful runs / scheduled | p99 job duration | on-time completions / total |
| Data pipeline | records processed / received | end-to-end latency | valid outputs / total |
| Storage | successful reads+writes / total | p99 read latency | data integrity checks passed |

### Error Budget Math
```
SLO Target:        99.9%
Error Budget:      0.1% of requests can fail
Monthly requests:  1,000,000
Monthly budget:    1,000 errors allowed

If 30-day window:
  Daily budget:    ~33 errors/day
  Hourly budget:   ~1.4 errors/hour

Burn rate:
  1x burn  = consuming budget at exactly the sustainable rate
  2x burn  = will exhaust budget in 15 days
  10x burn = will exhaust budget in 3 days
  100x burn = will exhaust budget in 7.2 hours
```

### Error Budget Policies
| Budget Remaining | Action |
|-----------------|--------|
| > 50% | Normal development velocity, deploy freely |
| 25-50% | Increase monitoring, review recent changes |
| 10-25% | Slow deployments, prioritize reliability work |
| < 10% | Freeze non-essential deploys, all hands on reliability |
| Exhausted | Full deploy freeze until budget recovers |

---

## AWS Infrastructure Patterns

### Compute Health Checks
```bash
# EC2 instance status (system + instance checks)
aws ec2 describe-instance-status \
  --filters "Name=instance-status.status,Values=impaired" \
  --query 'InstanceStatuses[*].[InstanceId,InstanceStatus.Status,SystemStatus.Status]'

# ECS service stability (running vs desired)
aws ecs describe-services --cluster CLUSTER \
  --services SERVICE \
  --query 'services[*].[serviceName,runningCount,desiredCount,deployments[*].rolloutState]'

# EKS node health
kubectl get nodes -o wide
kubectl top nodes

# Lambda errors (last hour)
aws cloudwatch get-metric-statistics \
  --namespace AWS/Lambda --metric-name Errors \
  --dimensions Name=FunctionName,Value=FUNCTION \
  --period 3600 --statistics Sum \
  --start-time $(date -u -d '1 hour ago' +%Y-%m-%dT%H:%M:%S) \
  --end-time $(date -u +%Y-%m-%dT%H:%M:%S)
```

### Database Health
```bash
# RDS CPU and connections
aws cloudwatch get-metric-statistics \
  --namespace AWS/RDS --metric-name CPUUtilization \
  --dimensions Name=DBInstanceIdentifier,Value=INSTANCE \
  --period 300 --statistics Average

# RDS free storage
aws cloudwatch get-metric-statistics \
  --namespace AWS/RDS --metric-name FreeStorageSpace \
  --dimensions Name=DBInstanceIdentifier,Value=INSTANCE \
  --period 300 --statistics Minimum

# DynamoDB throttled requests
aws cloudwatch get-metric-statistics \
  --namespace AWS/DynamoDB --metric-name ThrottledRequests \
  --dimensions Name=TableName,Value=TABLE \
  --period 300 --statistics Sum
```

### Networking
```bash
# ALB 5xx errors
aws cloudwatch get-metric-statistics \
  --namespace AWS/ApplicationELB --metric-name HTTPCode_ELB_5XX_Count \
  --period 300 --statistics Sum

# Target group health
aws elbv2 describe-target-health --target-group-arn TARGET_GROUP_ARN

# VPC Flow Logs (rejected traffic)
aws logs filter-log-events \
  --log-group-name "vpc-flow-logs" \
  --filter-pattern "REJECT" --max-items 20
```

### Cost Queries
```bash
# Top 10 services by cost (MTD)
aws ce get-cost-and-usage \
  --time-period Start=$(date +%Y-%m-01),End=$(date +%Y-%m-%d) \
  --granularity MONTHLY --metrics BlendedCost \
  --group-by Type=DIMENSION,Key=SERVICE

# Reserved instance utilization
aws ce get-reservation-utilization \
  --time-period Start=$(date -d '30 days ago' +%Y-%m-%d),End=$(date +%Y-%m-%d)

# Savings Plans utilization
aws ce get-savings-plans-utilization \
  --time-period Start=$(date -d '30 days ago' +%Y-%m-%d),End=$(date +%Y-%m-%d)
```

---

## Google Cloud Infrastructure Patterns

### Compute Health Checks
```bash
# Instance status
gcloud compute instances list \
  --filter="status!=RUNNING" \
  --format="table(name,zone,status,lastStartTimestamp)"

# GKE cluster health
gcloud container clusters list \
  --format="table(name,status,currentMasterVersion,currentNodeCount)"

# GKE pod issues
kubectl get pods --all-namespaces \
  --field-selector=status.phase!=Running,status.phase!=Succeeded

# Cloud Run revisions
gcloud run revisions list \
  --format="table(name,active,service,status.conditions.status)"

# Cloud Functions errors
gcloud logging read \
  'resource.type="cloud_function" severity>=ERROR' \
  --limit=20 --format="table(timestamp,severity,textPayload)"
```

### Database Health
```bash
# Cloud SQL status and CPU
gcloud sql instances list \
  --format="table(name,state,databaseVersion,settings.tier)"

# Cloud SQL metrics via monitoring
gcloud monitoring time-series list \
  --filter='metric.type="cloudsql.googleapis.com/database/cpu/utilization"' \
  --interval-start-time=$(date -u -d '1 hour ago' +%Y-%m-%dT%H:%M:%SZ)

# Firestore usage
gcloud firestore operations list --format="table(name,state)" 2>/dev/null

# Memorystore (Redis) info
gcloud redis instances list --format="table(name,state,memorySizeGb,host)"
```

### Networking
```bash
# Load balancer health
gcloud compute backend-services get-health BACKEND_SERVICE --global 2>/dev/null

# SSL certificate expiry
gcloud compute ssl-certificates list \
  --format="table(name,type,expireTime)"

# Cloud Armor (WAF) denied requests
gcloud logging read \
  'resource.type="http_load_balancer" jsonPayload.enforcedSecurityPolicy.outcome="DENY"' \
  --limit=20
```

### Cost Queries
```bash
# Billing summary (requires billing export to BigQuery)
# Alternative: list active resources for manual review
gcloud compute instances list --format="table(name,machineType,zone,status)"
gcloud compute disks list --format="table(name,sizeGb,status,users)"

# Committed use discounts
gcloud compute commitments list \
  --format="table(name,status,plan,startTimestamp,endTimestamp)"
```

---

## Incident Response Framework

### Severity Levels
| Level | Definition | Response Time | Examples |
|-------|-----------|---------------|---------|
| SEV1 | Complete outage, data loss risk | Immediate | Service down, database corruption, security breach |
| SEV2 | Major degradation, significant user impact | 15 min | 50%+ errors, p99 > 10x normal, partial outage |
| SEV3 | Minor degradation, limited user impact | 1 hour | Elevated errors, single-AZ issue, non-critical service |
| SEV4 | Potential issue, no current user impact | Next business day | Warning threshold crossed, capacity approaching |

### Incident Timeline Template
```
[HH:MM UTC] DETECTED   — [How the issue was found]
[HH:MM UTC] TRIAGED    — [Severity assigned, responders identified]
[HH:MM UTC] DIAGNOSED  — [Root cause hypothesis]
[HH:MM UTC] MITIGATED  — [Immediate fix applied]
[HH:MM UTC] RESOLVED   — [Full resolution confirmed]
[HH:MM UTC] POSTMORTEM — [Review scheduled]
```

### Common Root Causes
| Symptom | Likely Causes | Investigation Steps |
|---------|--------------|-------------------|
| Sudden 5xx spike | Bad deploy, dependency failure, resource exhaustion | Check recent deploys, upstream health, resource metrics |
| Gradual latency increase | Memory leak, connection pool exhaustion, growing dataset | Check memory trends, connection counts, query plans |
| Intermittent errors | Network flapping, DNS issues, rate limiting | Check network metrics, DNS resolution, throttle logs |
| Capacity saturation | Traffic spike, inefficient query, resource leak | Check traffic patterns, slow queries, resource trends |
| Certificate errors | Expired cert, misconfigured cert, CA issue | Check cert expiry, chain validity, OCSP status |

### Remediation Risk Matrix
| Action | Risk | Requires Approval |
|--------|------|------------------|
| Restart pod/container | Low | No (if auto_remediate) |
| Scale up replicas | Low | No (if auto_remediate) |
| Clear disk space (logs/tmp) | Low | No (if auto_remediate) |
| Rollback deployment | Medium | Yes |
| Modify security group/firewall | High | Yes |
| Database failover | High | Yes |
| DNS change | High | Yes |
| IAM policy change | Critical | Yes |

---

## Cost Optimization Patterns

### Compute Right-Sizing
| Signal | Recommendation |
|--------|---------------|
| CPU avg < 10% for 14 days | Downsize instance type |
| CPU avg > 80% for 7 days | Upsize or add autoscaling |
| Memory usage < 20% | Consider smaller instance |
| Instance stopped > 7 days | Terminate and snapshot |
| Unattached EBS volumes | Delete or snapshot |
| Unassociated Elastic IPs | Release |

### Reserved Capacity Planning
```
Analyze last 90 days of usage:
- Steady-state instances → Reserved Instances / Committed Use
- Variable workloads → Savings Plans (AWS) / Sustained Use (GCP)
- Batch/fault-tolerant → Spot Instances / Preemptible VMs

Target: 60-70% reserved, 20-30% on-demand, 5-10% spot
```

### Database Cost Optimization
- Right-size RDS/Cloud SQL instances based on actual CPU/memory usage
- Use read replicas instead of scaling primary for read-heavy workloads
- Enable auto-pause for dev/staging databases
- Move infrequently accessed data to cheaper storage tiers
- Review and optimize expensive queries (slow query logs)

---

## Observability Best Practices

### Logging Levels
| Level | Use For | Alert On |
|-------|---------|----------|
| ERROR | Failures requiring attention | Yes (if rate exceeds threshold) |
| WARN | Potential problems, degraded paths | Aggregate only |
| INFO | Normal operations, request logs | Never |
| DEBUG | Troubleshooting detail | Never (enable temporarily) |

### Key Dashboards to Maintain
1. **Service Overview**: Golden signals for each service
2. **SLO Dashboard**: Current SLI values, error budget burn rate
3. **Infrastructure**: CPU, memory, disk, network per host/container
4. **Cost**: Daily spend, top services, anomaly detection
5. **Incidents**: Active incidents, MTTD, MTTR trends

### Alert Design Principles
- Alert on **symptoms** (user-facing impact), not **causes** (CPU high)
- Use **multi-window, multi-burn-rate** alerts for SLO-based alerting
- Every alert must have a **runbook** or link to investigation steps
- Aim for < 5% false positive rate — noisy alerts get ignored
- Page for SEV1/SEV2 only — everything else goes to a queue

---

## Kubernetes-Specific Patterns

### Pod Health Investigation
```bash
# Pods in bad state
kubectl get pods --all-namespaces --field-selector=status.phase!=Running,status.phase!=Succeeded

# Recent events (errors)
kubectl get events --sort-by='.lastTimestamp' --field-selector type=Warning | tail -20

# Pod resource usage vs limits
kubectl top pods --all-namespaces --sort-by=cpu | head -20

# OOMKilled pods
kubectl get pods --all-namespaces -o json | jq -r '.items[] | select(.status.containerStatuses[]?.lastState.terminated.reason == "OOMKilled") | .metadata.namespace + "/" + .metadata.name'

# Pending pods (scheduling issues)
kubectl get pods --all-namespaces --field-selector=status.phase=Pending
```

### Common K8s Issues
| Issue | Symptoms | Fix |
|-------|----------|-----|
| OOMKilled | Pod restarts, exit code 137 | Increase memory limits or fix leak |
| CrashLoopBackOff | Rapid restart cycling | Check logs, fix startup errors |
| ImagePullBackOff | Pod stuck pending | Fix image name/tag, check registry auth |
| Insufficient CPU/Memory | Pods pending | Scale node pool or reduce requests |
| Evicted | Pods terminated | Disk pressure — clean up or expand |

---

## Security Posture Checks

### Quick Security Audit
```bash
# AWS: Public S3 buckets
aws s3api list-buckets --query 'Buckets[*].Name' --output text | tr '\t' '\n' | while read bucket; do
  acl=$(aws s3api get-bucket-acl --bucket "$bucket" --query 'Grants[?Grantee.URI==`http://acs.amazonaws.com/groups/global/AllUsers`]' --output text 2>/dev/null)
  [ -n "$acl" ] && echo "PUBLIC: $bucket"
done

# AWS: Security groups with 0.0.0.0/0 ingress
aws ec2 describe-security-groups \
  --filters "Name=ip-permission.cidr,Values=0.0.0.0/0" \
  --query 'SecurityGroups[*].[GroupId,GroupName,IpPermissions[*].[FromPort,ToPort]]'

# GCP: Public firewall rules
gcloud compute firewall-rules list \
  --filter="sourceRanges=0.0.0.0/0 AND direction=INGRESS" \
  --format="table(name,allowed,targetTags)"

# Certificate expiry check
aws acm list-certificates --query 'CertificateSummaryList[*].[DomainName,NotAfter]' --output table 2>/dev/null
gcloud compute ssl-certificates list --format="table(name,expireTime)" 2>/dev/null
```
