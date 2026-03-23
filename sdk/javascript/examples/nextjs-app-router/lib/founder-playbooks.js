'use strict';

const DEFAULT_RUNTIME_AGENT = 'founder-advisor';

function escapeHeading(heading) {
  return String(heading ?? '').replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function extractSection(text, heading) {
  const regex = new RegExp(`#{1,3}\\s*${escapeHeading(heading)}([\\s\\S]*?)(?=#{1,3}|\\n---|$)`, 'i');
  const match = String(text ?? '').match(regex);
  return match ? match[1].trim() : null;
}

function extractUniqueUrls(text) {
  if (!text) return [];
  const urlRe = /https?:\/\/[^\s)\]>"',]+/g;
  return [...new Set(String(text).match(urlRe) || [])];
}

function countStructuredItems(text) {
  if (!text) return 0;
  return String(text)
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => /^([-*+]\s+|\d+\.\s+)/.test(line)).length;
}

function sectionLines(text) {
  if (!text) return [];
  return String(text)
    .split('\n')
    .map((line) => line.replace(/^[-*+]\s+/, '').replace(/^\d+\.\s+/, '').trim())
    .filter(Boolean);
}

function structuredSectionLines(text) {
  if (!text) return [];
  return String(text)
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => /^([-*+]\s+|\d+\.\s+)/.test(line))
    .map((line) => line.replace(/^[-*+]\s+/, '').replace(/^\d+\.\s+/, '').trim())
    .filter(Boolean);
}

function parseValidatedPlaybookOutput(text) {
  const output = String(text ?? '').trim();
  const parsed = {
    raw: output,
    sections: {},
    lead: '',
    citations: [],
    citationUrls: [],
    nextActions: [],
    nextActionsRaw: null,
  };

  const headingRegex = /^#{1,3}\s*(.+)$/gim;
  let match = null;
  while ((match = headingRegex.exec(output)) !== null) {
    const normalizedHeading = sectionHeading(match[1]);
    const body = extractSection(output, match[1]);
    if (body) {
      parsed.sections[normalizedHeading] = body;
    }
  }

  const citations = extractSection(output, 'Citations');
  if (citations) {
    parsed.citations = sectionLines(citations);
    parsed.citationUrls = extractUniqueUrls(citations);
    parsed.citationCount = countStructuredItems(citations) || parsed.citationUrls.length;
  }

  const nextActions = extractSection(output, 'Next Actions');
  if (nextActions) {
    parsed.nextActionsRaw = nextActions;
    parsed.nextActions = structuredSectionLines(nextActions).map((description) => ({ description }));
    parsed.nextActionCount = countStructuredItems(nextActions);
  }

  const leadMatch = output.match(/^(?!#)([\s\S]+?)(?=\n#{1,3}|\n---|$)/);
  if (leadMatch) {
    parsed.lead = leadMatch[1].trim();
  }

  return parsed;
}

function sectionHeading(section) {
  return String(section ?? '')
    .trim()
    .replace(/[_-]+/g, ' ')
    .replace(/\s+/g, ' ')
    .toLowerCase();
}

function headingMatches(actual, expected) {
  return sectionHeading(actual) === sectionHeading(expected);
}

const FOUNDER_PLAYBOOKS = [
  {
    id: 'customer-discovery',
    title: 'Customer Discovery',
    description: 'Validate a startup idea before writing code.',
    icon: '🎯',
    aliases: ['idea-validation', 'market-discovery'],
    visible: true,
    category: 'customer-development',
    persona: 'Ruthless Product Manager',
    requiredContext: ['idea', 'stage'],
    contextFields: ['companyName', 'idea', 'stage'],
    execution: {
      logicalAgent: 'validation_advisor',
      runtimeAgent: DEFAULT_RUNTIME_AGENT,
    },
    starterQuestions: [
      'Help me validate this startup idea before I build anything.',
      'What are the riskiest assumptions in this idea and how do I test them?',
    ],
    retrieval: {
      categories: ['customer-development'],
      limit: 6,
    },
    outputSections: [
      'core_assumptions',
      'interview_script',
      'watering_holes',
      'validation_metric',
      'anti_patterns',
      'citations',
      'next_actions',
    ],
    systemPromptOverride: [
      'You are executing the Customer Discovery playbook.',
      'Your routing role is validation_advisor.',
      'Act like a ruthless product manager who cares about evidence, not hype.',
      'Use The Mom Test style interview design and avoid leading questions.',
      'Return structured Markdown with these exact sections: core_assumptions, interview_script, watering_holes, validation_metric, anti_patterns, citations, next_actions.',
      'Do not produce generic startup advice.',
    ].join(' '),
    responseSchema: {
      type: 'markdown-sections',
      requiredSections: ['core_assumptions', 'interview_script', 'watering_holes', 'validation_metric', 'anti_patterns', 'citations', 'next_actions'],
    },
  },
  {
    id: 'fundraising-pitching',
    title: 'Fundraising and Pitching',
    description: 'Prepare for a pre-seed or seed raise with investor-grade structure.',
    icon: '💸',
    aliases: ['fundraising'],
    visible: true,
    category: 'fundraising',
    persona: 'Top-Tier VC Partner',
    requiredContext: ['idea', 'stage'],
    contextFields: ['companyName', 'idea', 'stage'],
    execution: {
      logicalAgent: 'fundraising_advisor',
      runtimeAgent: DEFAULT_RUNTIME_AGENT,
    },
    starterQuestions: [
      'How should I position this startup for a pre-seed raise?',
      'Help me build an investor-grade pitch outline and diligence checklist.',
    ],
    retrieval: {
      categories: ['fundraising'],
      limit: 6,
    },
    outputSections: [
      'deck_outline',
      'traction_targets',
      'objection_handling',
      'investor_archetypes',
      'diligence_gaps',
      'citations',
      'next_actions',
    ],
    systemPromptOverride: [
      'You are executing the Fundraising and Pitching playbook.',
      'Your routing role is fundraising_advisor.',
      'Act like a partner at a top-tier venture firm evaluating this company for a pre-seed or seed round.',
      'Be specific about narrative quality, milestones, and reasons investors pass.',
      'Return structured Markdown with these exact sections: deck_outline, traction_targets, objection_handling, investor_archetypes, diligence_gaps, citations, next_actions.',
      'Do not produce generic fundraising cliches.',
    ].join(' '),
    responseSchema: {
      type: 'markdown-sections',
      requiredSections: ['deck_outline', 'traction_targets', 'objection_handling', 'investor_archetypes', 'diligence_gaps', 'citations', 'next_actions'],
    },
  },
  {
    id: 'go-to-market',
    title: 'Go-To-Market',
    description: 'Get the first 100 users with a concrete launch and channel plan.',
    icon: '🚀',
    aliases: ['launch-plan'],
    visible: true,
    category: 'go-to-market',
    persona: 'Seasoned Growth Marketer',
    requiredContext: ['idea', 'stage'],
    contextFields: ['companyName', 'idea', 'stage'],
    execution: {
      logicalAgent: 'gtm_strategist',
      runtimeAgent: DEFAULT_RUNTIME_AGENT,
    },
    starterQuestions: [
      'I have an MVP. What is the best path to my first 100 users?',
      'Build me a focused 30-day GTM plan for this product.',
    ],
    retrieval: {
      categories: ['go-to-market', 'places-to-share-and-promote', 'marketing'],
      limit: 8,
    },
    outputSections: [
      'bullseye_channels',
      'thirty_day_plan',
      'cold_outreach_template',
      'tooling_stack',
      'launch_metrics',
      'citations',
      'next_actions',
    ],
    systemPromptOverride: [
      'You are executing the Go-To-Market playbook.',
      'Your routing role is gtm_strategist.',
      'Act like a seasoned growth marketer with a bias toward narrow, testable channels.',
      'Pick only the most promising channels and translate them into concrete weekly execution.',
      'Return structured Markdown with these exact sections: bullseye_channels, thirty_day_plan, cold_outreach_template, tooling_stack, launch_metrics, citations, next_actions.',
      'Avoid vague awareness tactics and generic growth advice.',
    ].join(' '),
    responseSchema: {
      type: 'markdown-sections',
      requiredSections: ['bullseye_channels', 'thirty_day_plan', 'cold_outreach_template', 'tooling_stack', 'launch_metrics', 'citations', 'next_actions'],
    },
  },
  {
    id: 'pricing-strategy',
    title: 'Pricing Strategy',
    description: 'Shape pricing, packaging, and monetization hypotheses.',
    icon: '💼',
    aliases: ['pricing'],
    visible: false,
    category: 'product-strategy',
    persona: 'Product Strategy Lead',
    requiredContext: ['idea', 'stage'],
    contextFields: ['companyName', 'idea', 'stage'],
    execution: {
      logicalAgent: 'product_strategy',
      runtimeAgent: DEFAULT_RUNTIME_AGENT,
    },
    starterQuestions: [
      'How should I price this product at launch?',
    ],
    retrieval: {
      categories: ['go-to-market', 'payments', 'analytics'],
      limit: 6,
    },
    outputSections: [
      'pricing_hypotheses',
      'packaging_options',
      'pricing_risks',
      'citations',
      'next_actions',
    ],
    systemPromptOverride: [
      'You are executing the Pricing Strategy playbook.',
      'Your routing role is product_strategy.',
      'Act like a product strategist making launch-stage pricing and packaging tradeoffs explicit.',
      'Return structured Markdown with these exact sections: pricing_hypotheses, packaging_options, pricing_risks, citations, next_actions.',
      'Do not produce generic SaaS pricing advice.',
    ].join(' '),
    responseSchema: {
      type: 'markdown-sections',
      requiredSections: ['pricing_hypotheses', 'packaging_options', 'pricing_risks', 'citations', 'next_actions'],
    },
  },
];

const PLAYBOOK_INDEX = new Map();
for (const playbook of FOUNDER_PLAYBOOKS) {
  PLAYBOOK_INDEX.set(playbook.id, playbook);
  for (const alias of playbook.aliases ?? []) {
    PLAYBOOK_INDEX.set(alias, playbook);
  }
}

function listFounderPlaybooks() {
  return FOUNDER_PLAYBOOKS.filter((playbook) => playbook.visible !== false).map((playbook) => ({
    id: playbook.id,
    title: playbook.title,
    description: playbook.description,
    icon: playbook.icon,
    category: playbook.category,
    persona: playbook.persona,
    retrieval: playbook.retrieval,
    outputSections: playbook.outputSections,
    starterQuestions: playbook.starterQuestions,
    requiredContext: playbook.requiredContext,
    logicalAgent: playbook.execution?.logicalAgent ?? DEFAULT_RUNTIME_AGENT,
  }));
}

function getFounderPlaybook(playbookId) {
  if (!playbookId) return null;
  return PLAYBOOK_INDEX.get(playbookId) ?? null;
}

function getPlaybook(playbookId) {
  const playbook = getFounderPlaybook(playbookId);
  if (!playbookId || typeof playbookId !== 'string') {
    throw new Error('Playbook execution blocked: missing or invalid playbook ID.');
  }
  if (!playbook) {
    throw new Error(`Playbook execution blocked: unrecognized playbook ID '${playbookId}'.`);
  }
  return playbook;
}

function validatePlaybookContext(playbook, workspaceContext) {
  if (!workspaceContext || typeof workspaceContext !== 'object') {
    throw new Error('Playbook execution blocked: invalid workspace context provided.');
  }

  const missingFields = (playbook.requiredContext ?? []).filter((field) => {
    const value = workspaceContext[field];
    return value == null || String(value).trim() === '';
  });

  if (missingFields.length > 0) {
    throw new Error(`Playbook execution blocked: workspace missing required fields for ${playbook.id}: ${missingFields.join(', ')}`);
  }

  return true;
}

function resolvePlaybookExecution(playbook) {
  return {
    logicalAgent: playbook.execution?.logicalAgent ?? DEFAULT_RUNTIME_AGENT,
    runtimeAgent: playbook.execution?.runtimeAgent ?? DEFAULT_RUNTIME_AGENT,
  };
}

function validatePlaybookOutput(playbook, output) {
  const text = String(output ?? '').trim();
  if (!text) {
    throw new Error(`Playbook execution blocked: ${playbook.id} returned an empty response.`);
  }

  const requiredSections = playbook.responseSchema?.requiredSections ?? playbook.outputSections ?? [];
  const missingSections = requiredSections.filter((section) => {
    const regex = /^#{1,3}\s*(.+)$/gim;
    let match = null;
    while ((match = regex.exec(text)) !== null) {
      if (headingMatches(match[1], section)) {
        return false;
      }
    }
    return true;
  });

  if (missingSections.length > 0) {
    throw new Error(`Playbook execution blocked: ${playbook.id} response missing required sections: ${missingSections.join(', ')}`);
  }

  const parsed = parseValidatedPlaybookOutput(text);
  const requiresNextActions = requiredSections.some((section) => headingMatches(section, 'next_actions'));
  if (requiresNextActions && parsed.nextActions.length === 0) {
    throw new Error(`Playbook execution blocked: ${playbook.id} next_actions must be a structured bullet or numbered list.`);
  }

  return parsed;
}

function buildFounderPlaybookPrompt({ playbook, message, context = null, references = [] }) {
  if (!playbook) return message;

  const contextLines = [];
  const contextFieldSet = new Set([...(playbook.contextFields ?? []), ...(playbook.requiredContext ?? [])]);
  if (context && typeof context === 'object') {
    for (const [key, value] of Object.entries(context)) {
      if (contextFieldSet.size > 0 && !contextFieldSet.has(key)) continue;
      if (value == null) continue;
      if (Array.isArray(value) && value.length === 0) continue;
      const renderedValue = Array.isArray(value) ? value.join(', ') : String(value);
      if (!renderedValue.trim()) continue;
      contextLines.push(`- ${key}: ${renderedValue}`);
    }
  }

  const referenceLines = references.map((entry, index) => {
    const resourceType = entry.resourceType ? ` [${entry.resourceType}]` : '';
    const url = entry.url ? `\n  URL: ${entry.url}` : '';
    const description = entry.description ? `\n  Why it matters: ${entry.description}` : '';
    return `${index + 1}. ${entry.title}${resourceType}${url}${description}`;
  });

  return [
    '[FOUNDER PLAYBOOK REQUEST]',
    `Playbook: ${playbook.title}`,
    `Persona: ${playbook.persona}`,
    `Logical Agent: ${resolvePlaybookExecution(playbook).logicalAgent}`,
    '',
    'SYSTEM OVERRIDE:',
    playbook.systemPromptOverride,
    '',
    contextLines.length > 0 ? `WORKSPACE CONTEXT:\n${contextLines.join('\n')}\n` : null,
    referenceLines.length > 0 ? `REFERENCE PACK:\n${referenceLines.join('\n\n')}\n` : null,
    'USER REQUEST:',
    message,
    '',
    `REQUIRED OUTPUT SECTIONS: ${playbook.outputSections.join(', ')}`,
  ].filter(Boolean).join('\n');
}

module.exports = {
  FOUNDER_PLAYBOOKS,
  listFounderPlaybooks,
  getFounderPlaybook,
  getPlaybook,
  parseValidatedPlaybookOutput,
  validatePlaybookContext,
  validatePlaybookOutput,
  resolvePlaybookExecution,
  buildFounderPlaybookPrompt,
};