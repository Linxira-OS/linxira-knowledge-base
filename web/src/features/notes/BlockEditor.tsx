/**
 * Notes and Wiki share one block editor. The implementation lives under
 * `features/editor/` so the Wiki surface can consume it too; this module is the
 * notes-facing entry point.
 */
export { BlockEditor, type BlockEditorProps, type EditorView } from '../editor/BlockEditor'
