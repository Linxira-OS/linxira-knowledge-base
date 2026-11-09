ALTER TABLE messages ADD COLUMN rewritten_query TEXT;
ALTER TABLE messages ADD COLUMN conversation_context_hash TEXT;
ALTER TABLE usage_events ADD COLUMN rewritten_query TEXT;
