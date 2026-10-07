// Framing calibrated against the user's 960×540 Felipe recording.
// 0.45 at the scene's 900px reference height gives 10.8px per meter.
export const PLAY_ZOOM = 0.45;

export function terrainFraming(slope: number, width: number, height: number) {
  const t = Math.max(0, Math.min(1, (Math.atan(Math.max(0, slope)) - Math.PI / 4) / (Math.PI / 6)));
  const steep = t * t * (3 - 2 * t);
  return {
    zoom: PLAY_ZOOM * (1 - 0.28 * steep),
    playerX: width * (0.2 + 0.12 * steep),
    baseline: height * (0.46 + 0.25 * steep),
  };
}

// Integrate a first-order follower for a target moving linearly during dt.
// Unlike lerping toward the frame's endpoint, this preserves tracking lag at
// different display refresh rates.
function follow(value: number, from: number, to: number, dt: number, tau: number) {
  const velocity = (to - from) / dt;
  return to - velocity * tau + (value - from + velocity * tau) * Math.exp(-dt / tau);
}

export class FollowCamera {
  x = 0;
  y = 0;
  private targetX = 0;
  private targetY = 0;
  private leadInitialized = false;

  reset(x: number, y: number) {
    this.x = this.targetX = x;
    this.y = this.targetY = y;
    this.leadInitialized = false;
  }

  update(x: number, y: number, vx: number, _supported: boolean, dt: number) {
    if (dt <= 0) return;
    // Keep forward anticipation continuous through takeoff, walls and landing.
    // Switching it off in air made the scenery brake every time Jump was pressed.
    const tauX = 0.28;
    if (!this.leadInitialized) {
      this.targetX += vx * tauX;
      this.leadInitialized = true;
    }
    const targetX = x + vx * tauX;
    this.x = follow(this.x, this.targetX, targetX, dt, tauX);
    this.y = follow(this.y, this.targetY, y, dt, 0.22);
    this.targetX = targetX;
    this.targetY = y;
  }
}
