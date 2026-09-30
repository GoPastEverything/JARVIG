/** Research systems stay off unless a host turns a flag on for a measured experiment. */
export interface FeatureFlags {
  readonly virtualGeometry: boolean;
  readonly proceduralMicrogeometry: boolean;
  readonly representationVirtualization: boolean;
  readonly serverMeshing: boolean;
  readonly advancedGi: boolean;
}

export const defaultFeatureFlags: FeatureFlags = {
  virtualGeometry: false,
  proceduralMicrogeometry: false,
  representationVirtualization: false,
  serverMeshing: false,
  advancedGi: false,
};

export function resolveFeatureFlags(override?: Partial<FeatureFlags>): FeatureFlags {
  return {
    virtualGeometry: override?.virtualGeometry ?? defaultFeatureFlags.virtualGeometry,
    proceduralMicrogeometry: override?.proceduralMicrogeometry ?? defaultFeatureFlags.proceduralMicrogeometry,
    representationVirtualization:
      override?.representationVirtualization ?? defaultFeatureFlags.representationVirtualization,
    serverMeshing: override?.serverMeshing ?? defaultFeatureFlags.serverMeshing,
    advancedGi: override?.advancedGi ?? defaultFeatureFlags.advancedGi,
  };
}
