#!/bin/bash
# Weekly Learning Consolidation Script
# Run this to review and consolidate memory learnings into skill files

MEMORY_DIR="$HOME/.claude/projects/-Users-ro-openfang/memory"
SKILLS_DIR="$HOME/Downloads/Resuming Context and Reviewing Handoff Documentation/skills"

echo "=== OpenFang Learning Consolidation ==="
echo ""
echo "Memory files to review:"
echo ""

# List feedback files
if [ -d "$MEMORY_DIR" ]; then
    echo "## Feedback Learnings:"
    ls -la "$MEMORY_DIR"/feedback_*.md 2>/dev/null || echo "  (none)"
    echo ""

    echo "## Project Context:"
    ls -la "$MEMORY_DIR"/project_*.md 2>/dev/null || echo "  (none)"
    echo ""

    echo "## References:"
    ls -la "$MEMORY_DIR"/reference_*.md 2>/dev/null || echo "  (none)"
fi

echo ""
echo "=== Available Skills to Update ==="
ls -d "$SKILLS_DIR"/*/ 2>/dev/null | xargs -n1 basename

echo ""
echo "=== Actions ==="
echo "1. Review each feedback_*.md file"
echo "2. If the learning is mature and reusable, add it to the appropriate skill's SKILL.md"
echo "3. Update .skill_versions.json if significant"
echo ""
echo "To open a memory file: cat $MEMORY_DIR/feedback_<name>.md"
echo "To open a skill file: cat $SKILLS_DIR/<skill>/SKILL.md"