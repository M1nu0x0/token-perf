// Hash routing: #/ , #/sessions?q=&sort= , #/s/<session-id>
export type Route = {
  name: 'overview' | 'sessions' | 'session';
  id?: string;
  q?: string;
  sort?: string;
  sub?: boolean;
};

function parse(hash: string): Route {
  const [path, query] = hash.replace(/^#\/?/, '').split('?');
  const params = new URLSearchParams(query);
  if (path.startsWith('s/')) return { name: 'session', id: decodeURIComponent(path.slice(2)) };
  if (path === 'sessions') {
    return {
      name: 'sessions',
      q: params.get('q') ?? undefined,
      sort: params.get('sort') ?? undefined,
      sub: params.get('sub') === '1',
    };
  }
  return { name: 'overview' };
}

export const route: Route = $state(parse(location.hash));

addEventListener('hashchange', () => {
  const next = parse(location.hash);
  Object.assign(route, { id: undefined, q: undefined, sort: undefined, sub: undefined }, next);
});

export const go = (hash: string) => {
  location.hash = hash;
};
