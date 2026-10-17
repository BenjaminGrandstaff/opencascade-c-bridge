import {createBoundaryControls} from './sketch-boundaries.mjs';
import {createStudioOperationEditor} from './sketch-studio-profile.mjs';
export function createStudioBoundaryEditor(options) {
  return createStudioOperationEditor({...options, createControls: createBoundaryControls,
    tool: 'boundary', name: 'material boundary editor',
    stagedMessage: 'Material boundaries staged. Preview checks closed loops, holes, regions and dependent solids.'});
}
