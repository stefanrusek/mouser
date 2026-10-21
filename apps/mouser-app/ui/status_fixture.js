// A `status` result for the UI tests (contracts/status.schema.json shape).

export function makeStatus({ permissions = {}, desk = null, computers = [], problems = [] } = {}) {
  return {
    version: '0.1.0',
    computer: { id: 'aa', name: 'Desk Mac' },
    state: 'active',
    desk,
    active_computer: null,
    computers: [
      {
        id: 'aa',
        name: 'Desk Mac',
        os: 'macos',
        version: '0.1.0',
        connection: 'self',
        permissions,
        limitations: [],
        displays: 2,
      },
      ...computers,
    ],
    problems,
  };
}

export const problem = (id, message, fix = null, cleared_at = null) => ({
  problem_id: id,
  kind: 'missing_permission',
  computer: 'aa',
  message,
  fix,
  raised_at: '2026-10-21T00:00:00Z',
  cleared_at,
});
