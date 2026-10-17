import {createPatternControls} from './pattern-controls.mjs';
import {createStudioOperationEditor} from './sketch-studio-profile.mjs';

export function createStudioPatternEditor(options) {
  return createStudioOperationEditor({...options, createControls: createPatternControls,
    tool: 'pattern', name: 'pattern editor',
    stagedMessage: 'Pattern staged. Preview checks generated copies, references and dependent solids.'});
}
