'use client';
import { getChoicesForCategory } from '../config/ai-choice-catalog';
import AiChoiceCard from '../cards/AiChoiceCard';

const PROMPT_CATEGORIES = [
  { category: 'prompt',  label: 'Prompting / idea generation', showAlways: true },
  { category: 'script',  label: 'Script writing',               showAlways: false, videoOnly: true },
  { category: 'image',   label: 'Image AI',                     showAlways: false, imageOnly: true },
  { category: 'video',   label: 'Video AI',                     showAlways: false, videoOnly: true },
  { category: 'voice',   label: 'Voice / sound',                showAlways: false, videoOnly: true },
];

export default function CreativeWizardStepAiChoices({ state, onChange, needsImage, needsVideo }) {
  function handleChoiceSelect(category, id) {
    onChange('ai_choices', { ...state.ai_choices, [category]: id });
  }

  const visibleGroups = PROMPT_CATEGORIES.filter(g => {
    if (g.showAlways) return true;
    if (g.imageOnly && needsImage) return true;
    if (g.videoOnly && needsVideo) return true;
    return false;
  });

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 28 }}>
      <div>
        <div style={{ fontWeight: 700, fontSize: 15, marginBottom: 4 }}>
          Choose your AI tools
        </div>
        <div style={{ fontSize: 13, color: 'var(--text-dim)' }}>
          Everything is set to &ldquo;Auto-recommend&rdquo; by default — change any group if you want more control.
          Image and video are separate lanes with separate approvals.
        </div>
      </div>

      {visibleGroups.map(group => {
        const choices = getChoicesForCategory(group.category);
        const selected = state.ai_choices[group.category] ?? '';
        return (
          <div key={group.category}>
            <div style={{ fontWeight: 600, fontSize: 13, color: 'var(--text-secondary)', marginBottom: 10, textTransform: 'uppercase', letterSpacing: '0.05em' }}>
              {group.label}
            </div>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))', gap: 10 }}>
              {choices.map(c => (
                <AiChoiceCard
                  key={c.id}
                  choice={c}
                  selected={selected === c.id}
                  onSelect={id => handleChoiceSelect(group.category, id)}
                />
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}
