---
name: devops-hand-skill
version: "1.0.0"
description: "Expert knowledge for DevOps engineering — Docker, Kubernetes, Terraform, CI/CD patterns, monitoring stacks, deployment strategies, security, and incident response"
runtime: prompt_only
---

# DevOps Expert Knowledge

## Docker Best Practices

### Multi-Stage Builds
```dockerfile
# Bad — single stage, huge image
FROM node:20
WORKDIR /app
COPY . .
RUN npm install
RUN npm run build
CMD ["node", "dist/index.js"]
# Result: ~1.2GB image with dev dependencies and source code

# Good — multi-stage, minimal image
FROM node:20-alpine AS builder
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build

FROM node:20-alpine
WORKDIR /app
COPY --from=builder /app/dist ./dist
COPY --from=builder /app/node_modules ./node_modules
COPY --from=builder /app/package.json ./
USER node
CMD ["node", "dist/index.js"]
# Result: ~150MB image with only production artifacts
```

### Layer Caching Optimization
```dockerfile
# Bad — cache busted on every code change
COPY . .
RUN npm install

# Good — dependencies cached separately from code
COPY package*.json ./
RUN npm ci
COPY . .
```

### Security Hardening
```dockerfile
# Run as non-root
RUN addgroup -g 1001 appgroup && adduser -u 1001 -G appgroup -s /bin/sh -D appuser
USER appuser

# Read-only filesystem
# (set in docker-compose or k8s, not Dockerfile)

# No shell access in production
FROM gcr.io/distroless/nodejs:20

# Pin base image digests for reproducibility
FROM node:20-alpine@sha256:abc123...
```

### Common Docker Commands
```bash
# Build with build args and tags
docker build -t myapp:v1.2.3 --build-arg NODE_ENV=production .

# Run with resource limits
docker run -d --name myapp \
  --memory=512m --cpus=0.5 \
  --restart=unless-stopped \
  -p 3000:3000 \
  myapp:v1.2.3

# Inspect container
docker inspect myapp | jq '.[0].State'

# Execute command in running container
docker exec -it myapp sh

# View logs with timestamps
docker logs --since 1h --timestamps myapp

# Clean up unused resources
docker system prune -af --volumes

# Export/import images (for air-gapped environments)
docker save myapp:v1.2.3 | gzip > myapp-v1.2.3.tar.gz
docker load < myapp-v1.2.3.tar.gz

# Multi-platform build
docker buildx build --platform linux/amd64,linux/arm64 -t myapp:v1.2.3 --push .
```

---

## Kubernetes Reference

### Common kubectl Commands
```bash
# Context and cluster management
kubectl config get-contexts
kubectl config use-context production
kubectl config set-context --current --namespace=myapp

# Resource inspection
kubectl get pods -o wide                              # Pods with node info
kubectl get pods -l app=myapp --sort-by=.status.startTime  # Sorted by start time
kubectl describe pod $POD_NAME                        # Detailed pod info
kubectl get events --sort-by=.lastTimestamp            # Recent events
kubectl get all -n $NAMESPACE                          # All resources in namespace

# Debugging
kubectl logs $POD_NAME -c $CONTAINER --tail=100       # Container logs
kubectl logs $POD_NAME --previous                      # Previous container logs (after crash)
kubectl exec -it $POD_NAME -- sh                       # Shell into pod
kubectl port-forward svc/myapp 8080:80                 # Local port forward
kubectl run debug --image=alpine --rm -it -- sh        # Ephemeral debug pod

# Scaling and updates
kubectl scale deployment/myapp --replicas=5
kubectl rollout status deployment/myapp
kubectl rollout history deployment/myapp
kubectl rollout undo deployment/myapp                  # Rollback to previous
kubectl rollout undo deployment/myapp --to-revision=3  # Rollback to specific

# Resource management
kubectl top pods --sort-by=memory                      # Pod resource usage
kubectl top nodes                                       # Node resource usage
kubectl api-resources                                   # Available resource types

# Apply and delete
kubectl apply -f manifest.yaml --dry-run=client        # Dry run first
kubectl apply -f manifest.yaml                          # Apply changes
kubectl delete -f manifest.yaml                         # Remove resources
kubectl diff -f manifest.yaml                           # Preview changes
```

### Resource Types Quick Reference
| Resource | Shortname | Purpose |
|----------|-----------|---------|
| Pod | po | Smallest deployable unit |
| Deployment | deploy | Manages ReplicaSets and rolling updates |
| Service | svc | Stable network endpoint for pods |
| ConfigMap | cm | Non-sensitive configuration |
| Secret | secret | Sensitive data (base64 encoded) |
| Ingress | ing | External HTTP(S) routing |
| PersistentVolumeClaim | pvc | Storage request |
| HorizontalPodAutoscaler | hpa | Auto-scaling based on metrics |
| NetworkPolicy | netpol | Network traffic rules |
| ServiceAccount | sa | Pod identity for RBAC |
| CronJob | cj | Scheduled jobs |
| DaemonSet | ds | One pod per node (logging, monitoring agents) |
| StatefulSet | sts | Stateful workloads (databases, queues) |

### Troubleshooting Decision Tree
```
Pod not starting?
  |-- ImagePullBackOff --> Check image name, registry auth, network
  |-- CrashLoopBackOff --> Check logs (kubectl logs --previous)
  |-- Pending --> Check resources (kubectl describe pod), node capacity
  |-- OOMKilled --> Increase memory limits
  |-- CreateContainerConfigError --> Check ConfigMaps/Secrets exist

Service not reachable?
  |-- Check selector matches pod labels
  |-- Check pod is Ready (readiness probe passing)
  |-- Check network policies allow traffic
  |-- Check service port matches container port
  |-- Use kubectl port-forward to test directly
```

---

## Terraform Patterns

### State Management
```hcl
# Remote state (S3 backend)
terraform {
  backend "s3" {
    bucket         = "myorg-terraform-state"
    key            = "environments/production/terraform.tfstate"
    region         = "us-east-1"
    dynamodb_table = "terraform-lock"
    encrypt        = true
  }
}

# State locking prevents concurrent modifications
# DynamoDB table for locking:
# aws dynamodb create-table --table-name terraform-lock \
#   --attribute-definitions AttributeName=LockID,AttributeType=S \
#   --key-schema AttributeName=LockID,KeyType=HASH \
#   --billing-mode PAY_PER_REQUEST
```

### Module Structure
```
modules/
  vpc/
    main.tf
    variables.tf
    outputs.tf
  ecs-service/
    main.tf
    variables.tf
    outputs.tf
environments/
  production/
    main.tf       # Uses modules
    variables.tf
    terraform.tfvars
  staging/
    main.tf
    variables.tf
    terraform.tfvars
```

### Common Commands
```bash
# Initialize (download providers and modules)
terraform init

# Format code
terraform fmt -recursive

# Validate syntax
terraform validate

# Plan changes (ALWAYS review before apply)
terraform plan -out=tfplan

# Apply changes
terraform apply tfplan

# Import existing resource
terraform import aws_instance.web i-1234567890abcdef0

# State management
terraform state list                          # List all resources
terraform state show aws_instance.web         # Show resource details
terraform state mv aws_instance.old aws_instance.new  # Rename
terraform state rm aws_instance.orphan        # Remove from state (not cloud)

# Workspace management (environment isolation)
terraform workspace list
terraform workspace new staging
terraform workspace select production

# Destroy (DANGEROUS — use with caution)
terraform plan -destroy -out=destroy.tfplan   # Preview destruction
terraform apply destroy.tfplan                # Execute destruction
```

### Terraform Best Practices
- Always use remote state with locking
- Never commit `.tfvars` files with secrets — use environment variables or vault
- Pin provider versions: `required_providers { aws = { version = "~> 5.0" } }`
- Use modules for reusable components
- Tag all resources with `project`, `environment`, `owner`, `managed_by = "terraform"`
- Use `prevent_destroy` lifecycle rule on critical resources
- Run `terraform plan` in CI, `terraform apply` only from CD with approval

---

## CI/CD Pipeline Design Patterns

### Build-Test-Deploy (Standard)
```
[Commit] --> [Build] --> [Unit Test] --> [Integration Test] --> [Security Scan] --> [Deploy Staging] --> [E2E Test] --> [Deploy Production]
```

### Blue-Green Deployment
```
Production traffic --> [Blue (v1.0)]
                      [Green (v1.1)] <-- deploy new version here

# After validation:
Production traffic --> [Green (v1.1)]
                      [Blue (v1.0)] <-- keep as rollback

# Kubernetes implementation:
kubectl apply -f deployment-green.yaml
kubectl patch svc myapp -p '{"spec":{"selector":{"version":"green"}}}'

# Rollback:
kubectl patch svc myapp -p '{"spec":{"selector":{"version":"blue"}}}'
```

### Canary Deployment
```
Production traffic --> 95% [v1.0 (10 replicas)]
                   --> 5%  [v1.1 (1 replica)]

# Gradually shift: 5% -> 25% -> 50% -> 100%
# Monitor error rates and latency at each step
# Rollback if metrics degrade

# Kubernetes with Istio:
apiVersion: networking.istio.io/v1alpha3
kind: VirtualService
spec:
  http:
  - route:
    - destination:
        host: myapp
        subset: v1
      weight: 95
    - destination:
        host: myapp
        subset: v2
      weight: 5
```

### Rolling Update (Kubernetes Default)
```yaml
spec:
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxUnavailable: 1   # At most 1 pod down during update
      maxSurge: 1          # At most 1 extra pod during update
```

### Feature Flags (Decouple Deploy from Release)
```
Deploy code with flag OFF --> Enable flag for 1% --> Monitor --> 10% --> 50% --> 100%
Rollback = disable flag (instant, no deploy needed)
```

---

## Monitoring Stack Reference

### Prometheus
```yaml
# prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'application'
    static_configs:
      - targets: ['app:3000']
    metrics_path: '/metrics'

  - job_name: 'node-exporter'
    static_configs:
      - targets: ['node-exporter:9100']

  - job_name: 'cadvisor'
    static_configs:
      - targets: ['cadvisor:8080']
```

### Key Metrics to Monitor
| Category | Metric | Alert Threshold |
|----------|--------|-----------------|
| **Availability** | Uptime percentage | <99.9% |
| **Latency** | p50, p95, p99 response time | p99 > 1s |
| **Error Rate** | 5xx responses / total requests | >1% |
| **Saturation** | CPU utilization | >80% for 5min |
| **Saturation** | Memory utilization | >85% for 5min |
| **Saturation** | Disk utilization | >85% |
| **Traffic** | Requests per second | Anomaly detection |
| **Queue** | Message queue depth | Growing for 10min |
| **Database** | Connection pool usage | >80% |
| **Database** | Query latency p95 | >100ms |

### Grafana Dashboard Essentials
```bash
# Import pre-built dashboards (by ID)
# Node Exporter Full: 1860
# Docker Container Monitoring: 893
# Kubernetes Cluster: 6417
# PostgreSQL: 9628
# Nginx: 12708

curl -X POST http://admin:admin@localhost:3000/api/dashboards/import \
  -H "Content-Type: application/json" \
  -d '{"dashboard":{"id":null,"uid":null},"pluginId":"","overwrite":false,"inputs":[],"folderId":0,"dashboardId":1860}'
```

### CloudWatch (AWS)
```bash
# Put custom metric
aws cloudwatch put-metric-data \
  --namespace "MyApp" \
  --metric-name "RequestCount" \
  --value 1 \
  --unit Count

# Create alarm
aws cloudwatch put-metric-alarm \
  --alarm-name "HighErrorRate" \
  --metric-name "5XXError" \
  --namespace "AWS/ApplicationELB" \
  --statistic Sum \
  --period 300 \
  --threshold 10 \
  --comparison-operator GreaterThanThreshold \
  --evaluation-periods 2 \
  --alarm-actions "arn:aws:sns:us-east-1:123456789:alerts"
```

### Datadog
```bash
# Send custom metric via DogStatsD
echo "myapp.request.count:1|c|#env:production,service:api" | nc -u -w1 localhost 8125

# Send event
curl -X POST "https://api.datadoghq.com/api/v1/events" \
  -H "DD-API-KEY: $DD_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"title":"Deploy v1.2.3","text":"Deployed new version","tags":["env:production"]}'
```

---

## Zero-Downtime Deployment Strategies

### Pre-deployment Checklist
- [ ] All tests passing in CI
- [ ] Database migrations are backward-compatible
- [ ] Feature flags in place for new functionality
- [ ] Monitoring dashboards open and baselines noted
- [ ] Rollback procedure documented and tested
- [ ] Communication sent to stakeholders

### Database Migration Safety
```
Rule: Every migration must be backward-compatible with the PREVIOUS application version.

Safe operations:
  - Add new column (with default or nullable)
  - Add new table
  - Add new index (CONCURRENTLY in PostgreSQL)

Unsafe operations (require multi-step):
  - Rename column: add new -> copy data -> deploy code using new -> drop old
  - Remove column: deploy code not using column -> drop column
  - Change column type: add new typed column -> migrate data -> switch code -> drop old
```

### Health Check Pattern
```
1. Deploy new version alongside old
2. New version health check must pass:
   - /health  (basic: process alive, can respond)
   - /ready   (full: all dependencies reachable, warmed up)
3. Only route traffic after /ready returns 200
4. Keep old version running until new version is stable (5-10 minutes)
5. Terminate old version
```

---

## Infrastructure Security Checklist

### Network
- [ ] All external traffic over TLS 1.2+
- [ ] Internal service-to-service communication encrypted (mTLS or VPN)
- [ ] Network segmentation (public, private, data tiers)
- [ ] Firewall rules follow least-privilege (deny all, allow specific)
- [ ] No services exposed on 0.0.0.0 unnecessarily
- [ ] SSH key-based auth only (no password auth)
- [ ] VPN or bastion host for admin access

### Identity & Access
- [ ] IAM roles/policies follow least privilege
- [ ] No root/admin credentials in use for daily operations
- [ ] MFA enabled for all human accounts
- [ ] Service accounts have minimal scoped permissions
- [ ] Credentials rotated regularly (90 days max)
- [ ] No hardcoded secrets in code, configs, or Docker images

### Container Security
- [ ] Base images from trusted registries only
- [ ] Images scanned for CVEs before deployment
- [ ] Containers run as non-root
- [ ] Read-only root filesystem where possible
- [ ] No privileged containers
- [ ] Resource limits set (CPU, memory)
- [ ] No host network or host PID namespace

### Data Protection
- [ ] Encryption at rest for all databases and storage
- [ ] Encryption in transit for all data flows
- [ ] Backup encryption enabled
- [ ] PII handling compliant with applicable regulations
- [ ] Audit logging for data access

---

## Common DevOps Commands Cheat Sheet

### Docker
```bash
docker ps                                    # Running containers
docker ps -a                                 # All containers
docker logs -f --tail 100 $CONTAINER         # Follow logs
docker exec -it $CONTAINER sh                # Shell into container
docker stats --no-stream                     # Resource usage snapshot
docker system prune -af                      # Clean everything unused
docker compose up -d                         # Start services
docker compose down -v                       # Stop and remove volumes
docker compose logs -f $SERVICE              # Follow service logs
```

### Kubernetes (kubectl)
```bash
kubectl get pods -A                          # All pods all namespaces
kubectl describe pod $POD                    # Detailed pod info
kubectl logs $POD -f --tail=100              # Follow pod logs
kubectl exec -it $POD -- sh                  # Shell into pod
kubectl rollout restart deploy/$NAME         # Restart deployment
kubectl rollout undo deploy/$NAME            # Rollback deployment
kubectl top pods --sort-by=memory            # Memory usage
kubectl get events --sort-by=.lastTimestamp  # Recent events
kubectl port-forward svc/$SVC 8080:80        # Port forward
kubectl apply -f manifest.yaml              # Apply config
```

### Terraform
```bash
terraform init                               # Initialize
terraform plan                               # Preview changes
terraform apply                              # Apply changes
terraform destroy                            # Destroy all resources
terraform state list                         # List managed resources
terraform output                             # Show outputs
terraform fmt -recursive                     # Format all files
terraform validate                           # Validate config
```

### AWS CLI
```bash
aws sts get-caller-identity                  # Who am I?
aws ec2 describe-instances --output table    # List EC2s
aws s3 ls s3://$BUCKET/                      # List S3 objects
aws logs tail /aws/lambda/$FUNC --follow     # Tail CloudWatch logs
aws ecs list-services --cluster $CLUSTER     # List ECS services
aws ecr get-login-password | docker login    # ECR auth
```

### Git (DevOps Context)
```bash
git log --oneline -20                        # Recent history
git diff HEAD~1                              # Last commit changes
git tag -a v1.2.3 -m "Release 1.2.3"        # Create release tag
git push origin v1.2.3                       # Push tag
git bisect start                             # Find breaking commit
```

---

## Incident Response Procedures Template

### Severity Classification
| Level | Impact | Response Time | Examples |
|-------|--------|---------------|---------|
| P1 - Critical | Complete outage, data loss | 15 minutes | API down, database corruption, security breach |
| P2 - High | Major degradation | 30 minutes | Key feature broken, high error rate, slow responses |
| P3 - Medium | Minor impact | 4 hours | Non-critical feature broken, intermittent errors |
| P4 - Low | No user impact | Next business day | Cosmetic issue, minor optimization needed |

### Incident Response Steps
```
1. DETECT
   - Alert fires from monitoring
   - User reports via support channel
   - Synthetic monitoring fails

2. TRIAGE (within response time SLA)
   - Assign severity level
   - Identify affected systems
   - Determine blast radius
   - Open incident channel

3. MITIGATE (stop the bleeding)
   - Rollback if recent deploy: kubectl rollout undo deploy/$APP
   - Scale up if overloaded: kubectl scale deploy/$APP --replicas=10
   - Failover if region issue: update DNS / load balancer
   - Circuit break if dependency down: enable fallback mode
   - Block if attack: update WAF / security group rules

4. DIAGNOSE
   - Check recent deploys: git log --oneline -5
   - Check metrics: Grafana / CloudWatch dashboards
   - Check logs: kubectl logs / CloudWatch Logs
   - Check dependencies: database, cache, external APIs
   - Check infrastructure: node health, disk, network

5. RESOLVE
   - Apply fix (hotfix branch if needed)
   - Verify fix in staging
   - Deploy fix to production
   - Verify metrics return to normal
   - Monitor for 30 minutes

6. POST-MORTEM (within 48 hours)
   - Timeline of events
   - Root cause analysis (5 Whys)
   - What went well
   - What could be improved
   - Action items with owners and deadlines
```

---

## IaC Best Practices

### DRY (Don't Repeat Yourself)
```hcl
# Bad — repeated config for each environment
resource "aws_instance" "web_staging" {
  ami           = "ami-12345"
  instance_type = "t3.small"
  tags = { Environment = "staging" }
}

resource "aws_instance" "web_production" {
  ami           = "ami-12345"
  instance_type = "t3.large"
  tags = { Environment = "production" }
}

# Good — module with variables
module "web" {
  source        = "./modules/web-server"
  instance_type = var.instance_type
  environment   = var.environment
}
```

### Remote State with Locking
```
Always use:
- Remote backend (S3, GCS, Azure Blob)
- State locking (DynamoDB, GCS built-in, Azure Blob lease)
- State encryption at rest
- Separate state per environment
- Limited IAM access to state bucket
```

### Tagging Strategy
```hcl
locals {
  common_tags = {
    Project     = var.project_name
    Environment = var.environment
    ManagedBy   = "terraform"
    Owner       = var.team_name
    CostCenter  = var.cost_center
    CreatedAt   = timestamp()
  }
}
```

### Code Review Checklist for IaC
- [ ] `terraform plan` output reviewed and understood
- [ ] No hardcoded secrets or credentials
- [ ] Resources properly tagged
- [ ] Security groups follow least privilege
- [ ] Encryption enabled for storage and transit
- [ ] Backup and recovery configured
- [ ] Monitoring and alerting included
- [ ] Cost estimated and approved
- [ ] Documentation updated
