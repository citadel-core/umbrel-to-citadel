import { Head } from "$fresh/runtime.ts";

export default function Home() {
  return (
    <>
      <Head>
        <title>Upgrade to Citadel</title>
      </Head>
      <div class="p-4 w-screen h-screen flex items-center justify-center flex-col text-slate-800 dark:bg-slate-900 dark:text-white">
        <img
          src="/logo.svg"
          class="w-32 h-32"
          alt="the fresh logo: a sliced lemon dripping with juice"
        />
        <h2 class="mt-8 mb-2 font-bold font-serif text-5xl">
          Welcome to Citadel
        </h2>
        <p class="mb-6">
          Enjoy a fully free, open source node and reclaim control over your
          software
        </p>
        <button class="rounded p-3 bg-blue-300 dark:bg-blue-700">Get started</button>
      </div>
    </>
  );
}
