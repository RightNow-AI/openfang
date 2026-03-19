'use client';

const TASK_LABELS = {
  generate_moodboard_directions: { label: 'Build moodboard',      icon: '🎨' },
  generate_prompt_pack:          { label: 'Draft prompts',         icon: '✏️' },
  generate_script_strategy:      { label: 'Draft script',          icon: '📝' },
  generate_image_drafts:         { label: 'Generate images',       icon: '🖼' },
  generate_video_plan:           { label: 'Plan video',            icon: '🎬' },
  generate_voice_drafts:         { label: 'Generate voice',        icon: '🎙' },
  generate_full_creative_pack:   { label: 'Full creative pack',    icon: '🚀' },
};

export default function CreativeTaskLauncher({ projectStatus, availableTasks, runningTask, onLaunchTask }) {
  const tasks = availableTasks ?? Object.keys(TASK_LABELS);

  return (
    <div data-cy="task-launcher">
      <div style={{ fontWeight: 700, fontSize: 11, color: 'var(--text-dim,#888)', textTransform: 'uppercase', letterSpacing: 1, marginBottom: 10 }}>Launch a task</div>
      {tasks.map(taskType => {
        const { label, icon } = TASK_LABELS[taskType] ?? { label: taskType, icon: '▶' };
        const isRunning = runningTask === taskType;
        const anyRunning = !!runningTask;
        return (
          <button
            key={taskType}
            data-cy={`launch-task-${taskType}`}
            onClick={() => !anyRunning && onLaunchTask(taskType)}
            disabled={anyRunning}
            style={{
              display: 'flex',
              width: '100%',
              alignItems: 'center',
              gap: 8,
              padding: '7px 10px',
              borderRadius: 7,
              background: isRunning ? 'rgba(124,58,237,.12)' : 'transparent',
              border: `1px solid ${isRunning ? 'var(--accent,#7c3aed)' : 'var(--border,#333)'}`,
              color: isRunning ? 'var(--accent,#7c3aed)' : 'var(--text-primary,#f1f1f1)',
              cursor: anyRunning ? 'not-allowed' : 'pointer',
              fontSize: 12,
              marginBottom: 6,
              opacity: anyRunning && !isRunning ? 0.45 : 1,
              transition: 'opacity .15s, background .15s',
              textAlign: 'left',
            }}
          >
            <span>{isRunning ? '↻' : icon}</span>
            <span>{isRunning ? 'Running…' : label}</span>
          </button>
        );
      })}
    </div>
  );
}
