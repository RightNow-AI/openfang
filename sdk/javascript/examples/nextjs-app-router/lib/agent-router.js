/**
 * lib/agent-router.js
 *
 * Deterministic keyword-based router. Maps a user message to the best
 * internal specialist agent.
 *
 * Design: keep this simple and fast in sprint-1.  No LLM call needed.
 * Later, replace `select()` internals with an "alive-thinks" call to the
 * daemon when you want LLM-based dynamic routing.
 *
 * Routing rules are evaluated in order; first match wins.
 * If nothing matches, returns null — alive handles it directly.
 */

'use strict';

/** @typedef {{ match: string[], agent: string, reason: string }} RoutingRule */

/** @type {RoutingRule[]} */
const ROUTING_RULES = [
  {
    match: ['debug', 'bug', 'trace', 'error', 'crash', 'exception', 'stack trace', 'segfault', 'undefined', 'null pointer'],
    agent: 'debugger',
    reason: 'message references a bug or error',
  },
  {
    match: ['code', 'implement', 'refactor', 'function', 'class', 'script', 'program', 'algorithm', 'syntax', 'compile', 'python', 'javascript', 'typescript', 'rust', 'sql', 'hello world', 'snippet'],
    agent: 'coder',
    reason: 'message requests code implementation',
  },
  {
    match: ['review', 'pull request', 'pr ', 'code quality', 'lint', 'best practice', 'code smell', 'readability'],
    agent: 'code-reviewer',
    reason: 'message requests code review',
  },
  {
    match: ['research', 'compare', 'investigate', 'find out', 'look up', 'what is', 'explain', 'summarize', 'overview'],
    agent: 'researcher',
    reason: 'message requests research or explanation',
  },
  {
    match: ['plan', 'roadmap', 'sequence', 'steps to', 'outline', 'organize', 'prioritize', 'strategy', 'milestone'],
    agent: 'planner',
    reason: 'message requests planning or sequencing',
  },
  {
    match: ['write', 'draft', 'post', 'article', 'blog', 'email', 'document', 'readme', 'report', 'essay'],
    agent: 'writer',
    reason: 'message requests written content',
  },
];

const agentRouter = {
  /**
   * Select the best specialist for a user message.
   *
   * @param {{ message: string, availableAgents?: Array<{id: string}> }} opts
   * @returns {{ agent: string | null, reason: string }}
   */
  select({ message, availableAgents: _availableAgents }) {
    const lower = (message || '').toLowerCase();

    for (const rule of ROUTING_RULES) {
      const matched = rule.match.some((kw) => lower.includes(kw));
      if (!matched) continue;
      return { agent: rule.agent, reason: rule.reason };
    }

    return { agent: null, reason: 'no matching rule — alive answers directly' };
  },
};

module.exports = { agentRouter, ROUTING_RULES };
