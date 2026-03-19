// Creative Studio — shared types (JSDoc for editor hints, no runtime cost)

/**
 * @typedef {'image'|'video'|'image+video'} CreationType
 * @typedef {'ad'|'social'|'product-promo'|'explainer'|'lesson'|'brand'|'other'} CreativeGoal
 * @typedef {'draft'|'plan_ready'|'approved'|'running'|'done'|'error'} CreativeProjectStatus
 * @typedef {'pending'|'approved'|'rejected'} ApprovalState
 *
 * @typedef {Object} CreativeAiChoice
 * @property {string}  id
 * @property {string}  label
 * @property {string}  description
 * @property {string}  best_for
 * @property {'free'|'low'|'medium'|'high'} cost_tier
 * @property {'fast'|'medium'|'slow'} speed_label
 * @property {'prompt'|'image'|'video'|'voice'|'script'} category
 * @property {boolean} requires_approval
 * @property {boolean} auto_recommend
 *
 * @typedef {Object} CreativeAsset
 * @property {string} id
 * @property {'prompt'|'script'|'image'|'video'|'voice'|'final'} type
 * @property {string} label
 * @property {string|null} url
 * @property {string|null} content
 * @property {ApprovalState} approval_state
 *
 * @typedef {Object} CreativePlanStep
 * @property {string}  id
 * @property {string}  label
 * @property {string}  description
 * @property {boolean} requires_approval
 * @property {ApprovalState} approval_state
 * @property {'pending'|'running'|'done'|'error'} status
 *
 * @typedef {Object} CreativePlan
 * @property {CreativePlanStep[]} steps
 * @property {string[]}           ai_tools_used
 * @property {string[]}           expected_outputs
 *
 * @typedef {Object} CreativeProject
 * @property {string}              id
 * @property {string}              name
 * @property {CreationType}        creation_type
 * @property {CreativeGoal}        goal
 * @property {string}              topic
 * @property {string}              offer
 * @property {string}              audience
 * @property {string}              platform
 * @property {string}              desired_outcome
 * @property {string}              notes
 * @property {string}              style_description
 * @property {string[]}            visual_keywords
 * @property {string[]}            words_to_avoid
 * @property {string[]}            reference_links
 * @property {string|null}         aspect_ratio
 * @property {string|null}         duration
 * @property {string|null}         voice_tone
 * @property {Record<string,string>} ai_choices  keyed by category
 * @property {CreativePlan|null}   plan
 * @property {CreativeAsset[]}     assets
 * @property {CreativeProjectStatus} status
 * @property {string}              created_at
 * @property {string}              updated_at
 *
 * @typedef {Object} CreativeWizardState
 * @property {number}            step           1-based
 * @property {CreationType|''}   creation_type
 * @property {CreativeGoal|''}   goal
 * @property {string}            name
 * @property {string}            topic
 * @property {string}            offer
 * @property {string}            audience
 * @property {string}            platform
 * @property {string}            desired_outcome
 * @property {string}            notes
 * @property {string}            style_description
 * @property {string}            visual_keywords_raw
 * @property {string}            words_to_avoid_raw
 * @property {string}            reference_links_raw
 * @property {string}            aspect_ratio
 * @property {string}            duration
 * @property {string}            voice_tone
 * @property {Record<string,string>} ai_choices
 *
 * @typedef {Object} CreativeStarterTemplate
 * @property {string}            id
 * @property {string}            title
 * @property {string}            tagline
 * @property {string}            best_for
 * @property {CreationType}      creation_type
 * @property {CreativeGoal}      goal
 * @property {string[]}          ai_categories_needed
 * @property {Partial<CreativeWizardState>} wizard_defaults
 */

export {};
