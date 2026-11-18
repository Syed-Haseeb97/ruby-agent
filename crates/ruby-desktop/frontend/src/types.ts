export type OperationalState = "Idle" | "Listening" | "Thinking" | "Speaking" | "Acting" | "Error";

export type Emotion =
  | "Neutral"
  | "Happy"
  | "Excited"
  | "Sad"
  | "Empathetic"
  | "Curious"
  | "Surprised";

export interface OrbVisualState {
  operational: OperationalState;
  emotion: Emotion;
  thinking_spin_active: boolean;
  speaking_active: boolean;
}

export interface SequencedOrbEvent {
  seq: number;
  state: OrbVisualState;
}
