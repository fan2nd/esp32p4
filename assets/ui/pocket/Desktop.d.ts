import type { i32, Px, Color } from "@pocketjs/framework/vue-vapor/std";
export interface Item { id: i32; x: Px; y: Px; width: Px; height: Px; label: string; background: Color; border: Color; stroke: Px; radius: Px; }
export declare const items: Item[];
