/** A weapon's category is deliberately unconstrained until combat types are defined. */
export type WeaponType = string;

export interface Weapon {
  name: string;
  type: WeaponType;
  damage: number;
  weight: number;
}
