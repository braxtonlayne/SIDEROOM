-- D-746 (SEC-AUD-253): the Library stores a language as the Cinema does (D-719), and both store a script subtag in
-- title case. One function, `sr_core::language::normalise_tag`, is the rule; this migration rewrites what is stored
-- to it: the primary subtag in short form (`eng` -> `en`, `ger` and `deu` -> `de`; a code without a two-letter code
-- stays: `fil`), a 4-letter all-alphabetic subtag in title case (`zh-hant` -> `zh-Hant`), a 2-letter
-- all-alphabetic subtag in upper case (`pt-br` -> `pt-BR`), anything else as stored.
--
-- Rewritten: `reading_editions.language` (the only language the Library stores: the filter of the catalog compares
-- it, a submission's or an upload's `language` is a note in a JSON payload that is normalised when it becomes an
-- edition), and the Cinema columns 2970 rewrote (`video_titles.languages` and `subtitle_languages`, element by
-- element, order kept, a duplicate the rewrite makes removed; the `language_tag` of the audio and subtitle tracks;
-- the `language` of the trailers). Left alone, as in 2970: `video_*_tracks.language` (the code as probed).
--
-- THE COLLISION RULE: `reading_editions` is UNIQUE (series_id, language, label). Where the rewrite would give a
-- series two editions with the same language and label, nothing is merged and no chapter moves: the edition being
-- rewritten keeps its chapters and `is_default`, and its label becomes `<label> (<old code>)`, the label cut first
-- so the whole stays within 120 characters (`Official` -> `Official (eng)`). Editions are taken in id order, so
-- when two collide with each other the older keeps its label. Staff merge or rename such an edition by hand.
--
-- Only rows that need it are touched; `updated_at` does not move (this is not an edit by staff). The search index
-- holds no languages. Idempotent: a second run finds nothing to rewrite. The pairs are the same list as
-- `sr_core::language::LANGUAGE_PAIRS` and as 2970's (the test `the_sql_pairs_and_the_rust_table_are_the_same_list`
-- reads both files).

DROP TABLE IF EXISTS sr_lang_pairs;
CREATE TEMP TABLE sr_lang_pairs (long TEXT PRIMARY KEY, short TEXT NOT NULL);
INSERT INTO sr_lang_pairs (long, short) VALUES
        ('aar','aa'), ('abk','ab'), ('afr','af'), ('aka','ak'), ('alb','sq'), ('amh','am'),
        ('ara','ar'), ('arg','an'), ('arm','hy'), ('asm','as'), ('ava','av'), ('ave','ae'),
        ('aym','ay'), ('aze','az'), ('bak','ba'), ('bam','bm'), ('baq','eu'), ('bel','be'),
        ('ben','bn'), ('bih','bh'), ('bis','bi'), ('bod','bo'), ('bos','bs'), ('bre','br'),
        ('bul','bg'), ('bur','my'), ('cat','ca'), ('ces','cs'), ('cha','ch'), ('che','ce'),
        ('chi','zh'), ('chu','cu'), ('chv','cv'), ('cor','kw'), ('cos','co'), ('cre','cr'),
        ('cym','cy'), ('cze','cs'), ('dan','da'), ('deu','de'), ('div','dv'), ('dut','nl'),
        ('dzo','dz'), ('ell','el'), ('eng','en'), ('epo','eo'), ('est','et'), ('eus','eu'),
        ('ewe','ee'), ('fao','fo'), ('fas','fa'), ('fij','fj'), ('fin','fi'), ('fra','fr'),
        ('fre','fr'), ('fry','fy'), ('ful','ff'), ('geo','ka'), ('ger','de'), ('gla','gd'),
        ('gle','ga'), ('glg','gl'), ('glv','gv'), ('gre','el'), ('grn','gn'), ('guj','gu'),
        ('hat','ht'), ('hau','ha'), ('heb','he'), ('her','hz'), ('hin','hi'), ('hmo','ho'),
        ('hrv','hr'), ('hun','hu'), ('hye','hy'), ('ibo','ig'), ('ice','is'), ('ido','io'),
        ('iii','ii'), ('iku','iu'), ('ile','ie'), ('ina','ia'), ('ind','id'), ('ipk','ik'),
        ('isl','is'), ('ita','it'), ('jav','jv'), ('jpn','ja'), ('kal','kl'), ('kan','kn'),
        ('kas','ks'), ('kat','ka'), ('kau','kr'), ('kaz','kk'), ('khm','km'), ('kik','ki'),
        ('kin','rw'), ('kir','ky'), ('kom','kv'), ('kon','kg'), ('kor','ko'), ('kua','kj'),
        ('kur','ku'), ('lao','lo'), ('lat','la'), ('lav','lv'), ('lim','li'), ('lin','ln'),
        ('lit','lt'), ('ltz','lb'), ('lub','lu'), ('lug','lg'), ('mac','mk'), ('mah','mh'),
        ('mal','ml'), ('mao','mi'), ('mar','mr'), ('may','ms'), ('mkd','mk'), ('mlg','mg'),
        ('mlt','mt'), ('mon','mn'), ('mri','mi'), ('msa','ms'), ('mya','my'), ('nau','na'),
        ('nav','nv'), ('nbl','nr'), ('nde','nd'), ('ndo','ng'), ('nep','ne'), ('nld','nl'),
        ('nno','nn'), ('nob','nb'), ('nor','no'), ('nya','ny'), ('oci','oc'), ('oji','oj'),
        ('ori','or'), ('orm','om'), ('oss','os'), ('pan','pa'), ('per','fa'), ('pli','pi'),
        ('pol','pl'), ('por','pt'), ('pus','ps'), ('que','qu'), ('roh','rm'), ('ron','ro'),
        ('rum','ro'), ('run','rn'), ('rus','ru'), ('sag','sg'), ('san','sa'), ('sin','si'),
        ('slk','sk'), ('slo','sk'), ('slv','sl'), ('sme','se'), ('smo','sm'), ('sna','sn'),
        ('snd','sd'), ('som','so'), ('sot','st'), ('spa','es'), ('sqi','sq'), ('srd','sc'),
        ('srp','sr'), ('ssw','ss'), ('sun','su'), ('swa','sw'), ('swe','sv'), ('tah','ty'),
        ('tam','ta'), ('tat','tt'), ('tel','te'), ('tgk','tg'), ('tgl','tl'), ('tha','th'),
        ('tib','bo'), ('tir','ti'), ('ton','to'), ('tsn','tn'), ('tso','ts'), ('tuk','tk'),
        ('tur','tr'), ('twi','tw'), ('uig','ug'), ('ukr','uk'), ('urd','ur'), ('uzb','uz'),
        ('ven','ve'), ('vie','vi'), ('vol','vo'), ('wel','cy'), ('wln','wa'), ('wol','wo'),
        ('xho','xh'), ('yid','yi'), ('yor','yo'), ('zha','za'), ('zho','zh'), ('zul','zu');

-- A tag in the stored form (`sr_core::language::normalise_tag`), for a tag that is already shaped like one; the
-- primary subtag of anything else is left as it is.
CREATE FUNCTION pg_temp.sr_norm_tag(tag TEXT) RETURNS TEXT
    LANGUAGE sql STABLE AS
$$
    SELECT CASE
        WHEN split_part(tag, '-', 1) ~ '^[A-Za-z]{2,3}$' THEN
            coalesce((SELECT p.short FROM sr_lang_pairs p WHERE p.long = lower(split_part(tag, '-', 1))),
                     lower(split_part(tag, '-', 1)))
        ELSE split_part(tag, '-', 1)
    END
    || coalesce((
        SELECT string_agg('-' || CASE
                   WHEN s ~ '^[A-Za-z]{4}$' THEN upper(left(s, 1)) || lower(substr(s, 2))
                   WHEN s ~ '^[A-Za-z]{2}$' THEN upper(s)
                   ELSE s
               END, '' ORDER BY ord)
          FROM unnest(string_to_array(tag, '-')) WITH ORDINALITY AS u(s, ord)
         WHERE ord > 1), '')
$$;

-- A list with every element in the stored form: order kept (first place of each value), duplicates removed.
CREATE FUNCTION pg_temp.sr_norm_list(tags TEXT[]) RETURNS TEXT[]
    LANGUAGE sql STABLE AS
$$
    SELECT coalesce(array_agg(v ORDER BY first_ord), '{}')
      FROM (SELECT pg_temp.sr_norm_tag(e) AS v, min(ord) AS first_ord
              FROM unnest(tags) WITH ORDINALITY AS u(e, ord)
             GROUP BY 1) s
$$;

-- The Library: one edition at a time, in id order, so the collision rule sees what the earlier rewrites did.
DO $$
DECLARE
    r RECORD;
    new_lang TEXT;
    new_label TEXT;
    suffix TEXT;
BEGIN
    FOR r IN
        SELECT id, series_id, language, label
          FROM reading_editions
         WHERE pg_temp.sr_norm_tag(language) <> language
         ORDER BY id
    LOOP
        new_lang := pg_temp.sr_norm_tag(r.language);
        new_label := r.label;
        IF EXISTS (SELECT 1 FROM reading_editions x
                    WHERE x.series_id = r.series_id AND x.id <> r.id
                      AND x.language = new_lang AND x.label = new_label) THEN
            suffix := ' (' || r.language || ')';
            new_label := left(r.label, 120 - char_length(suffix)) || suffix;
            IF EXISTS (SELECT 1 FROM reading_editions x
                        WHERE x.series_id = r.series_id AND x.id <> r.id
                          AND x.language = new_lang AND x.label = new_label) THEN
                -- The bracketed label is taken too (it was already there): the edition's id is unique.
                suffix := ' (' || r.language || ' ' || r.id || ')';
                new_label := left(r.label, 120 - char_length(suffix)) || suffix;
            END IF;
        END IF;
        UPDATE reading_editions SET language = new_lang, label = new_label WHERE id = r.id;
    END LOOP;
END
$$;

-- The Cinema: the columns 2970 rewrote.
UPDATE video_titles
   SET languages = pg_temp.sr_norm_list(languages)
 WHERE languages <> pg_temp.sr_norm_list(languages);

UPDATE video_titles
   SET subtitle_languages = pg_temp.sr_norm_list(subtitle_languages)
 WHERE subtitle_languages <> pg_temp.sr_norm_list(subtitle_languages);

UPDATE video_audio_tracks
   SET language_tag = pg_temp.sr_norm_tag(language_tag)
 WHERE language_tag IS NOT NULL AND pg_temp.sr_norm_tag(language_tag) <> language_tag;

UPDATE video_subtitle_tracks
   SET language_tag = pg_temp.sr_norm_tag(language_tag)
 WHERE language_tag IS NOT NULL AND pg_temp.sr_norm_tag(language_tag) <> language_tag;

UPDATE video_trailers
   SET language = pg_temp.sr_norm_tag(language)
 WHERE language IS NOT NULL AND pg_temp.sr_norm_tag(language) <> language;

DROP FUNCTION pg_temp.sr_norm_list(TEXT[]);
DROP FUNCTION pg_temp.sr_norm_tag(TEXT);
DROP TABLE sr_lang_pairs;
