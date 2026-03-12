# Software Engineering (SWE) Agent Documentation

## Overview

The Software Engineering (SWE) Agent is OpenFang's specialized autonomous agent for performing software development tasks. Unlike traditional code assistants that wait for user prompts, the SWE Agent processes software engineering tasks submitted via API endpoints or automatically routed by the Supervisor Engine.

## Key Features

### Supported Operations
- **Read File** (`ReadFile`): Safely read file contents with character limits
- **Write File** (`WriteFile`): Create or update file contents with backup and dry-run options  
- **Execute Command** (`ExecuteCommand`): Execute shell commands in a sandboxed environment
- **Task Chaining**: Chain multiple file operations and commands into coherent workflows

### Dashboard Integration
The "Software Engineer" tab in the OpenFang dashboard provides:
- Real-time task status visualization
- Progress tracking with event streaming
- Output preview (truncated to 200 characters for security)
- Cancel and retry capabilities per task
- Task history and analytics

## API Endpoints

### `/api/swe/tasks`
- `POST` - Create new SWE task
- `GET` - List all tasks with summary views

### `/api/swe/tasks/{id}`
- `GET` - Get detailed task information
- `DELETE` - Delete/cancel task

### `/api/swe/tasks/{id}/cancel`
- `POST` - Cancel running or pending task

### `/api/swe/tasks/{id}/events`
- `GET` - Get detailed task events log with content previews

## A2A Integration

The SWE Agent integrates with the OpenFang A2A (Agent-to-Agent) system:

### Automatic Task Routing
- The Supervisor Engine automatically detects software engineering tasks based on keywords in the task description
- Tasks containing keywords like "code", "implement", "debug", "fix", "refactor", "test" are automatically routed to the SWE Agent
- Full MAESTRO pipeline orchestration continues for non-SWE tasks

### Direct Delegation
- Use `/api/supervisor/delegate` to explicitly route tasks to the SWE Agent 
- The API endpoint bypasses automatic detection in favor of intentional SWE processing

## Task Life Cycle

### Task States
- `Pending`: Task created but not started
- `Running`: Task in execution, events being processed
- `Completed`: Task finished successfully
- `Failed`: Task completed with errors
- `Cancelled`: Task interrupted by user or system

### Event Categories
- `FileRead(path, content)`: File operation event with content preview
- `FileWritten(path)`: Successful file write confirmation 
- `CommandExecuted(command, output, exit_code)`: Command execution results

## Security Considerations

### Sandboxing
- Command execution is limited to read-only by default
- File operations restricted to project directory scope
- Network access controlled by firewall rules
- Resource limits enforced (CPU, memory, time)

### Safe Operations
- Read operations limited to 1MB per file
- Write operations create backups before modifying
- Command execution limited to white-listed binaries
- Path traversal prevention for all file operations

## Best Practices

### Writing Tasks
- Be specific about file paths and command objectives
- Include sample code or pseudocode if available
- Specify format expectations for generated output
- Plan complex tasks in stages for better error recovery

### Monitoring
- Track task failure rates to identify common issue patterns
- Review logs for command execution side effects
- Monitor resource usage during heavy processing
- Test changes in development before production execution

## Use Cases

### Automated Code Generation
- Generate boilerplate code from specifications
- Create data processing scripts from requirements
- Implement unit tests based on function definitions

### System Administration  
- Automate file organization and cleanup tasks
- Generate configuration files from templates
- Execute deployment scripts and package managers

### Research & Debugging
- Download and analyze open-source code repositories
- Run debugging commands and analyze their output  
- Archive and categorize code examples by functionality