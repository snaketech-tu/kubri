<script lang="ts">
	import { t } from '../i18n';
	import logo from '../assets/logo.svg';
	import { session } from '../session.svelte';
	import Phrase from './Phrase.svelte';
	import { coreText } from '../i18n/errors';
	import { errorMessage } from '../format';

	let password = $state('');
	let error = $state<string | null>(signInError());
	let busy = $state(false);
	let input = $state<HTMLInputElement>();

	$effect(() => {
		if (!busy) input?.focus();
	});

	/** Why signing in with the provider failed: the server sends the browser back with it. */
	function signInError(): string | null {
		const url = new URL(location.href);
		const message = url.searchParams.get('sign-in-error');
		if (message !== null) {
			url.searchParams.delete('sign-in-error');
			history.replaceState(history.state, '', url);
		}
		return message;
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		if (password === '') {
			error = t('Enter the password.');
			input?.focus();
			return;
		}
		busy = true;
		error = null;
		try {
			await session.signIn(password);
		} catch (err) {
			error = errorMessage(err);
			password = '';
		} finally {
			busy = false;
		}
	}
</script>

<div class="page">
	{#if session.method === 'oidc'}
		<div class="card">
			<img src={logo} alt="" width="44" height="44" />
			<h1>{t('Sign in to Kubri')}</h1>
			<p class="intro">{t('This Kubri is shared by a team. You use it with your own permissions in the clusters.')}</p>
			{#if error}<p class="error" role="alert">{coreText(error)}</p>{/if}
			<a class="button" href="/api/auth/login">{session.provider ? t('Sign in with {provider}', { provider: session.provider }) : t('Sign in with your provider')}</a>
		</div>
	{:else}
		<form onsubmit={submit}>
			<img src={logo} alt="" width="44" height="44" />
			<h1>{t('Sign in to Kubri')}</h1>
			<!-- Lets password managers file the password under a name. -->
			<input type="text" name="username" autocomplete="username" value="kubri" hidden />
			<label>
				<span>{t('Password')}</span>
				<input
					bind:this={input}
					bind:value={password}
					type="password"
					name="password"
					autocomplete="current-password"
					disabled={busy}
					aria-invalid={error !== null}
					aria-describedby={error ? 'sign-in-error' : undefined}
				/>
			</label>
			{#if error}<p class="error" id="sign-in-error" role="alert">{error}</p>{/if}
			<button type="submit" disabled={busy}>{busy ? t('Signing in…') : t('Sign in')}</button>
			<p class="hint">
				<Phrase
					text={t('The password is set with {variable} when kubri-server starts. If none was set, the server made one up and printed it in its log.')}
					code={{ variable: 'KUBRI_PASSWORD' }}
				/>
			</p>
		</form>
	{/if}
</div>

<style>
	.page {
		display: grid;
		place-items: center;
		min-height: 100vh;
		padding: 24px;
	}
	form,
	.card {
		display: flex;
		flex-direction: column;
		gap: 14px;
		width: min(360px, 100%);
		padding: 32px 28px 24px;
		border: 1px solid var(--border);
		border-radius: 14px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
	}
	img {
		align-self: center;
	}
	h1 {
		margin: 0 0 4px;
		font-size: 19px;
		text-align: center;
		letter-spacing: -0.01em;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 6px;
		font-size: 13px;
		font-weight: 600;
	}
	label input {
		padding: 8px 10px;
		font-weight: 400;
	}
	label input[aria-invalid='true'] {
		border-color: var(--bad);
	}
	.error {
		margin: -4px 0 0;
		font-size: 13px;
		color: var(--bad);
	}
	.intro {
		margin: -4px 0 4px;
		font-size: 13.5px;
		line-height: 1.5;
		text-align: center;
		color: var(--text-muted);
	}
	button,
	.button {
		padding: 8px 12px;
		border: none;
		border-radius: 8px;
		background: var(--accent);
		color: var(--on-accent);
		font-weight: 600;
		text-align: center;
		text-decoration: none;
	}
	button:hover:not(:disabled),
	.button:hover {
		background: var(--accent-strong);
	}
	button:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.hint {
		margin: 4px 0 0;
		font-size: 12.5px;
		line-height: 1.5;
		color: var(--text-muted);
	}
	.hint :global(code) {
		font-family: var(--mono);
		font-size: 12px;
	}
</style>
