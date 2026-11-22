package dev.jadix.nativeui;

import android.text.*;
import android.text.style.*;
import org.json.*;
import java.util.*;
import java.util.function.Function;

/** Retains shared inert markup as editable ranges, including unsupported OS attributes. */
final class NativeRichMarkup {
    private static final class Element {
        final String open, close; final int order, parent;
        Element(JSONObject value, int order) { open = value.optString("open"); close = value.optString("close"); parent = value.optInt("parent", -1); this.order = order; }
        Element(String open, String close, int order) { this.open = open; this.close = close; this.order = order; parent = -1; }
    }
    private static final class Gap {}
    private static final class ImportedStyles {
        final Set<Object> spans = Collections.newSetFromMap(new IdentityHashMap<Object, Boolean>());
    }
    static Spanned parse(JSONObject plan) throws JSONException {
        JSONArray elements = plan.getJSONArray("elements");
        HashMap<Integer, Integer> starts = new HashMap<>();
        HashSet<Integer> opened = new HashSet<>();
        SpannableStringBuilder result = new SpannableStringBuilder(Html.fromHtml(preservePreWhitespace(plan.getString("taggedHtml")), Html.FROM_HTML_MODE_COMPACT, null, (opening, tag, text, reader) -> {
            if (tag.startsWith("jadixwhitespace")) {
                if (opening) text.append((char)Integer.parseInt(tag.substring(15)));
                return;
            }
            if (!tag.startsWith("jadixformat")) return;
            int index;
            try { index = Integer.parseInt(tag.substring(11)); } catch (NumberFormatException invalid) { return; }
            JSONObject value = elements.optJSONObject(index); if (value == null) return;
            if (opening) {
                // TagSoup can move/reopen unknown wrappers around block tags.
                // The shared plan retains the authored ancestry, so a sibling
                // starts after all unrelated open ranges have ended.
                if (!opened.add(index)) return;
                HashSet<Integer> ancestors = new HashSet<>();
                for (int parent = value.optInt("parent", -1); parent >= 0;) {
                    ancestors.add(parent);
                    JSONObject ancestor = elements.optJSONObject(parent);
                    parent = ancestor == null ? -1 : ancestor.optInt("parent", -1);
                }
                for (Integer active : new ArrayList<>(starts.keySet())) {
                    if (!ancestors.contains(active)) closeElement(text, elements.optJSONObject(active), active, starts.remove(active));
                }
                starts.put(index, text.length()); return;
            }
            Integer position = starts.remove(index); if (position == null) return;
            closeElement(text, value, index, position);
        }));
        for (Element element : result.getSpans(0, result.length(), Element.class)) {
            result.setSpan(element, result.getSpanStart(element), result.getSpanEnd(element), Spanned.SPAN_INCLUSIVE_INCLUSIVE);
        }
        ImportedStyles imported = new ImportedStyles();
        Collections.addAll(imported.spans, result.getSpans(0, result.length(), CharacterStyle.class));
        Collections.addAll(imported.spans, result.getSpans(0, result.length(), ParagraphStyle.class));
        result.setSpan(imported, 0, result.length(), Spanned.SPAN_INCLUSIVE_INCLUSIVE);
        return result;
    }
    private static String preservePreWhitespace(String html) {
        StringBuilder result = new StringBuilder(); boolean pre = false;
        java.util.regex.Matcher tokens = java.util.regex.Pattern.compile("<[^>]*>|[^<]+").matcher(html);
        while (tokens.find()) {
            String token = tokens.group();
            if (token.startsWith("<")) {
                if (token.matches("(?i)<pre(?:\\s[^>]*)?>")) pre = true;
                else if (token.equalsIgnoreCase("</pre>")) pre = false;
                result.append(token);
            } else if (pre) {
                for (int i = 0; i < token.length(); i++) {
                    char value = token.charAt(i);
                    if (value == ' ' || value == '\n' || value == '\r' || value == '\t')
                        result.append("<jadixwhitespace").append((int)value).append("></jadixwhitespace").append((int)value).append('>');
                    else result.append(value);
                }
            } else result.append(token);
        }
        return result.toString();
    }
    private static void closeElement(Editable text, JSONObject value, int index, int position) {
            int start = position, end = text.length();
            if (value.optBoolean("block") && !value.optString("open").startsWith("<pre")) {
                while (start < end && text.charAt(start) == '\n') start++;
                while (end > start && text.charAt(end - 1) == '\n') end--;
                if (start > position) text.setSpan(new Gap(), position, start, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
                if (end < text.length()) text.setSpan(new Gap(), end, text.length(), Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
            }
            // The importer keeps appending after a closing tag. Its end must
            // stay fixed until the whole document has been imported.
            if (!value.optString("open").isEmpty()) text.setSpan(new Element(value, index), start, end, Spanned.SPAN_INCLUSIVE_EXCLUSIVE);
    }
    static boolean contains(Spanned text) { return text.getSpans(0, text.length(), Element.class).length > 0; }
    static String export(Spanned text, Function<ImageSpan, String> imageHtml, float density) {
        // OS paste/edit operations can add formatting not represented by imported HTML.
        SpannableStringBuilder formatted = new SpannableStringBuilder(text);
        Set<Object> imported = Collections.newSetFromMap(new IdentityHashMap<Object, Boolean>());
        for (ImportedStyles item : text.getSpans(0, text.length(), ImportedStyles.class)) imported.addAll(item.spans);
        int order = 16384;
        for (CharacterStyle style : text.getSpans(0, text.length(), CharacterStyle.class)) {
            if (imported.contains(style)) continue;
            String open = null, close = "</span>";
            if (style instanceof StyleSpan) {
                int value = ((StyleSpan)style).getStyle();
                open = "<span style=\"" + ((value & android.graphics.Typeface.BOLD) != 0 ? "font-weight:bold;" : "")
                    + ((value & android.graphics.Typeface.ITALIC) != 0 ? "font-style:italic;" : "") + "\">";
            } else if (style instanceof UnderlineSpan) { open = "<u>"; close = "</u>";
            } else if (style instanceof StrikethroughSpan) { open = "<s>"; close = "</s>";
            } else if (style instanceof SuperscriptSpan) { open = "<sup>"; close = "</sup>";
            } else if (style instanceof SubscriptSpan) { open = "<sub>"; close = "</sub>";
            } else if (style instanceof ForegroundColorSpan) {
                open = String.format(Locale.ROOT, "<span style=\"color:#%06x\">", ((ForegroundColorSpan)style).getForegroundColor() & 0xffffff);
            } else if (style instanceof BackgroundColorSpan) {
                open = String.format(Locale.ROOT, "<span style=\"background-color:#%06x\">", ((BackgroundColorSpan)style).getBackgroundColor() & 0xffffff);
            } else if (style instanceof URLSpan) {
                String url = ((URLSpan)style).getURL();
                if (url != null && url.matches("(?is)(https?://|mailto:|tel:|#).*")) { open = "<a href=\"" + TextUtils.htmlEncode(url) + "\">"; close = "</a>"; }
            } else if (style instanceof TypefaceSpan) {
                String family = ((TypefaceSpan)style).getFamily();
                if (family != null && family.length() <= 256) { open = "<font face=\"" + TextUtils.htmlEncode(family) + "\">"; close = "</font>"; }
            } else if (style instanceof AbsoluteSizeSpan) {
                AbsoluteSizeSpan size = (AbsoluteSizeSpan)style;
                float value = size.getSize() / (size.getDip() ? 1 : Math.max(0.01f, density));
                if (Float.isFinite(value) && value > 0) open = "<span style=\"font-size:" + value + "px\">";
            } else if (style instanceof RelativeSizeSpan) {
                float value = ((RelativeSizeSpan)style).getSizeChange();
                if (Float.isFinite(value) && value > 0) open = "<span style=\"font-size:" + value + "em\">";
            }
            if (open != null) formatted.setSpan(new Element(open, close, order++), text.getSpanStart(style), text.getSpanEnd(style), Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
        }
        for (ParagraphStyle style : text.getSpans(0, text.length(), ParagraphStyle.class)) {
            if (imported.contains(style)) continue;
            String open = null, close = "</div>";
            if (style instanceof AlignmentSpan) {
                Layout.Alignment alignment = ((AlignmentSpan)style).getAlignment();
                String value = alignment == Layout.Alignment.ALIGN_CENTER ? "center"
                    : alignment == Layout.Alignment.ALIGN_OPPOSITE ? "end" : "start";
                open = "<div style=\"text-align:" + value + "\">";
            } else if (style instanceof BulletSpan) { open = "<ul><li>"; close = "</li></ul>";
            } else if (style instanceof QuoteSpan) { open = "<blockquote>"; close = "</blockquote>";
            } else if (style instanceof LeadingMarginSpan) {
                LeadingMarginSpan margin = (LeadingMarginSpan)style;
                float rest = margin.getLeadingMargin(false) / Math.max(0.01f, density);
                float first = margin.getLeadingMargin(true) / Math.max(0.01f, density);
                open = "<div style=\"margin-left:" + rest + "px;text-indent:" + (first - rest) + "px\">";
            }
            if (open == null) continue;
            int start = text.getSpanStart(style), limit = text.getSpanEnd(style);
            // Android paragraph spans can cover several lines; each list item has its own range.
            while (start < limit) {
                int end = start;
                while (end < limit && text.charAt(end) != '\n') end++;
                formatted.setSpan(new Element(open, close, order++), start, end, Spanned.SPAN_INCLUSIVE_INCLUSIVE);
                if (end < limit) formatted.setSpan(new Gap(), end, end + 1, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
                start = end + 1;
            }
        }
        return exportElements(formatted, imageHtml);
    }
    private static String exportElements(Spanned text, Function<ImageSpan, String> imageHtml) {
        HashMap<Integer, Element> identities = new HashMap<>();
        for (Element element : text.getSpans(0, text.length(), Element.class)) identities.put(element.order, element);
        TreeSet<Integer> boundaries = new TreeSet<>(); boundaries.add(0); boundaries.add(text.length());
        for (Object span : text.getSpans(0, text.length(), Object.class)) {
            if (span instanceof Element || span instanceof Gap || span instanceof ImageSpan) {
                boundaries.add(text.getSpanStart(span)); boundaries.add(text.getSpanEnd(span));
            }
        }
        StringBuilder html = new StringBuilder(); ArrayList<Element> stack = new ArrayList<>();
        Integer[] points = boundaries.toArray(new Integer[0]);
        for (int point = 0; point < points.length; point++) {
            int start = points[point], end = point + 1 < points.length ? points[point + 1] : start;
            ArrayList<Element> active = new ArrayList<>();
            for (Element element : text.getSpans(start, end, Element.class)) {
                int a = text.getSpanStart(element), b = text.getSpanEnd(element);
                if (a < b && a <= start && b >= end && end > start) active.add(element);
            }
            active.sort((a, b) -> {
                int first = Integer.compare(text.getSpanStart(a), text.getSpanStart(b));
                if (first != 0) return first;
                int last = Integer.compare(text.getSpanEnd(b), text.getSpanEnd(a));
                return last != 0 ? last : Integer.compare(a.order, b.order);
            });
            ArrayList<Element> empty = new ArrayList<>();
            for (Element element : text.getSpans(start, start, Element.class))
                if (text.getSpanStart(element) == start && text.getSpanEnd(element) == start) empty.add(element);
            empty.sort(Comparator.comparingInt(element -> element.order));
            for (Element element : empty) {
                ArrayList<Element> parents = new ArrayList<>();
                Element ancestor = element;
                while (ancestor != null) { parents.add(0, ancestor); ancestor = identities.get(ancestor.parent); }
                reconcile(html, stack, parents); stack = parents;
            }
            reconcile(html, stack, active);
            stack = active;
            if (start == end) continue;
            boolean gap = false;
            for (Gap item : text.getSpans(start, end, Gap.class)) if (text.getSpanStart(item) <= start && text.getSpanEnd(item) >= end) { gap = true; break; }
            if (gap) continue;
            boolean image = false;
            for (ImageSpan item : text.getSpans(start, end, ImageSpan.class)) {
                if (text.getSpanStart(item) == start && text.getSpanEnd(item) == end) {
                    String value = imageHtml.apply(item); if (value != null) { html.append(value); image = true; break; }
                }
            }
            if (!image) {
                String value = TextUtils.htmlEncode(text.subSequence(start, end).toString());
                boolean pre = false;
                for (Element element : active) if (element.open.matches("(?i)<pre(?:\\s[^>]*)?>")) { pre = true; break; }
                html.append(pre ? value : value.replace("\n", "<br>"));
            }
        }
        for (int i = stack.size() - 1; i >= 0; i--) html.append(stack.get(i).close);
        return html.toString();
    }
    private static void reconcile(StringBuilder html, List<Element> stack, List<Element> active) {
        int common = 0; while (common < stack.size() && common < active.size() && stack.get(common) == active.get(common)) common++;
        for (int i = stack.size() - 1; i >= common; i--) html.append(stack.get(i).close);
        for (int i = common; i < active.size(); i++) html.append(active.get(i).open);
    }
    private NativeRichMarkup() {}
}
