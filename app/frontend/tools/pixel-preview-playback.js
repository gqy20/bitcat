// Reduced motion quiets ambient playback without disconnecting a requested walk
// from its gait. Entry/exit actions are part of that same user-triggered motion.
export function previewFrame(pet, reducedMotion) {
  const requestedMotion = pet.state === 'walk' || pet.locomotionAction != null || pet.dragPhase != null;
  return reducedMotion && !requestedMotion ? 0 : pet.frame;
}
