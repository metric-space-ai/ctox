ALTER TABLE threads ADD COLUMN subagent_parent_thread_id TEXT;
ALTER TABLE threads ADD COLUMN subagent_depth INTEGER;
ALTER TABLE threads ADD COLUMN agent_path TEXT;

CREATE INDEX idx_threads_subagent_parent ON threads(subagent_parent_thread_id);
CREATE INDEX idx_threads_agent_path ON threads(agent_path);
