import { createClient } from "@supabase/supabase-js";

const runtime = window.degine || {};

export const supabaseUrl = runtime.supabaseUrl || import.meta.env.VITE_SUPABASE_URL;
export const supabaseKey = runtime.supabaseKey || import.meta.env.VITE_SUPABASE_PUBLISHABLE_KEY;

export const supabase = supabaseUrl && supabaseKey ? createClient(supabaseUrl, supabaseKey) : null;
