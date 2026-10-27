// Every section of the window, by the id its route names.

import * as desk from './desk.js';
import * as diagnostics from './diagnostics.js';
import * as displays from './displays.js';
import * as health from './health.js';
import * as help from './help.js';
import * as settings from './settings.js';
import * as welcome from './welcome.js';

export const REGISTRY = { desk, displays, health, settings, diagnostics, help, welcome };
